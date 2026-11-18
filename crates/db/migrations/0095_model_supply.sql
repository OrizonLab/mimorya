-- How each company's model is supplied (ADR-208): with its own key
-- (`client_key`, the default, with no row), with the operator's within a
-- monthly cap (`capped`), or against a prepaid credit (`prepaid`). The
-- operator alone writes these tables, with the owner role, through
-- `mimorya-admin`: the application role reads them, and nothing it runs can
-- give a company the operator's key. Amounts are in micro-dollars, as every
-- cost; the cap and the credit count re-invoiced amounts.
CREATE TABLE tenant_model_supply (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  formula text NOT NULL
    CONSTRAINT tenant_model_supply_formula CHECK (formula IN ('client_key', 'capped', 'prepaid')),
  -- From 1 to 100,000 USD a month, in the platform's time zone.
  monthly_cap_micros bigint
    CONSTRAINT tenant_model_supply_cap CHECK (monthly_cap_micros BETWEEN 1000000 AND 100000000000),
  -- Without a cap: only when the operator asks for it explicitly.
  unlimited boolean NOT NULL DEFAULT false,
  -- The markup of the re-invoiced amounts, from 0 to 100 %, in basis points.
  markup_bps integer NOT NULL DEFAULT 0
    CONSTRAINT tenant_model_supply_markup CHECK (markup_bps BETWEEN 0 AND 10000),
  updated_at timestamptz NOT NULL DEFAULT now(),
  -- A cap or the unlimited mode for the capped formula, neither otherwise.
  CONSTRAINT tenant_model_supply_capped CHECK (
    CASE WHEN formula = 'capped' THEN (monthly_cap_micros IS NULL) = unlimited
         ELSE monthly_cap_micros IS NULL AND NOT unlimited END
  )
);

ALTER TABLE tenant_model_supply ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_model_supply AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT ON tenant_model_supply TO mimorya_app;

-- The prepaid credit's register, in append only: each payment a non-zero
-- amount (negative for a correction) and its invoice's reference, unique in
-- the company so that the same is never recorded twice.
CREATE TABLE tenant_credit_entries (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  amount_micros bigint NOT NULL
    CONSTRAINT tenant_credit_entries_amount
    CHECK (amount_micros <> 0 AND amount_micros BETWEEN -1000000000000 AND 1000000000000),
  reference text NOT NULL
    CONSTRAINT tenant_credit_entries_reference CHECK (char_length(reference) BETWEEN 1 AND 100),
  recorded_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_credit_entries_reference_key UNIQUE (tenant_id, reference)
);

ALTER TABLE tenant_credit_entries ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_credit_entries AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT ON tenant_credit_entries TO mimorya_app;

CREATE INDEX tenant_credit_entries_tenant ON tenant_credit_entries (tenant_id, recorded_at);

-- An entry is never changed nor deleted, not even by the owner: a
-- correction is a new entry. A company deleted whole (ADR-192) takes its
-- register with it: that deletion runs inside its foreign key's trigger.
CREATE FUNCTION tenant_credit_entries_append_only() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF TG_OP = 'DELETE' AND pg_trigger_depth() > 1 THEN
    RETURN OLD;
  END IF;
  RAISE EXCEPTION 'credit register: an entry is never changed nor deleted; a correction is a new entry';
END
$$;

CREATE TRIGGER tenant_credit_entries_append_only
  BEFORE UPDATE OR DELETE ON tenant_credit_entries
  FOR EACH ROW EXECUTE FUNCTION tenant_credit_entries_append_only();

CREATE TRIGGER tenant_credit_entries_no_truncate
  BEFORE TRUNCATE ON tenant_credit_entries
  FOR EACH STATEMENT EXECUTE FUNCTION tenant_credit_entries_append_only();

-- The formula each call was made under: `client_key` on the company's own
-- key, else its formula on the operator's, with the amount re-invoiced
-- beside the real cost, never changed by the markup.
ALTER TABLE usage_records
  ADD COLUMN supply_formula text NOT NULL DEFAULT 'client_key'
    CONSTRAINT usage_records_supply_formula
    CHECK (supply_formula IN ('client_key', 'capped', 'prepaid')),
  ADD COLUMN billed_micros bigint;

-- The companies on the platform's model until now: no provider of their
-- own for their conversations. Their past calls were on the operator's key,
-- but for the reviews of a model they chose apart (ADR-108): counted under
-- the capped formula, at their real cost.
UPDATE usage_records u
   SET supply_formula = 'capped', billed_micros = u.cost_micros
 WHERE u.kind = 'llm'
   AND NOT EXISTS (
     SELECT 1 FROM tenant_llm_settings s
      WHERE s.tenant_id = u.tenant_id AND s.purpose = 'conversations'
   )
   AND NOT (
     u.role = 'review' AND EXISTS (
       SELECT 1 FROM tenant_llm_settings s
        WHERE s.tenant_id = u.tenant_id AND s.purpose = 'verification'
     )
   );

ALTER TABLE usage_records
  ADD CONSTRAINT usage_records_billed
  CHECK ((supply_formula = 'client_key') = (billed_micros IS NULL));

-- What a prepaid company consumed, read at each check of its balance.
CREATE INDEX usage_records_prepaid ON usage_records (tenant_id) INCLUDE (billed_micros)
  WHERE supply_formula = 'prepaid';

-- Those companies keep their access, capped: 100 USD a month, or twice what
-- they spent this month if more, rounded up to the dollar, so that none is
-- cut. The operator adjusts it.
INSERT INTO tenant_model_supply (tenant_id, formula, monthly_cap_micros)
SELECT t.id, 'capped',
       least(100000000000, greatest(100000000,
         (ceil(2 * coalesce(spent.billed, 0) / 1000000.0) * 1000000)::bigint))
  FROM tenants t
  LEFT JOIN LATERAL (
    SELECT sum(u.billed_micros) AS billed
      FROM usage_records u
     WHERE u.tenant_id = t.id
       AND u.supply_formula = 'capped'
       AND u.created_at >= date_trunc('month', now() AT TIME ZONE 'Europe/Paris')
                           AT TIME ZONE 'Europe/Paris'
  ) spent ON true
 WHERE NOT EXISTS (
   SELECT 1 FROM tenant_llm_settings s
    WHERE s.tenant_id = t.id AND s.purpose = 'conversations'
 );

-- Consumption is purged after five years (ADR-197): what a prepaid company
-- consumed in the calls purged is kept here, added in the purge's own
-- statement, so that its balance never rises by itself.
CREATE TABLE tenant_prepaid_purged (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  billed_micros bigint NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE tenant_prepaid_purged ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_prepaid_purged AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT ON tenant_prepaid_purged TO mimorya_app;

CREATE OR REPLACE FUNCTION system_purge_old_usage(p_batch integer)
RETURNS TABLE (tenant_id uuid, records integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH candidates AS (
    -- Each company's oldest, through its own index (tenant, time), then
    -- the oldest of them all, a batch at most, unlocked.
    SELECT picked.tenant_id, picked.id
      FROM tenants t
      CROSS JOIN LATERAL (
        SELECT u.tenant_id, u.id, u.created_at
          FROM usage_records u
         WHERE u.tenant_id = t.id
           AND u.created_at < now() - interval '5 years'
         ORDER BY u.created_at
         LIMIT least(greatest(p_batch, 1), 5000)
      ) picked
     ORDER BY picked.created_at
     LIMIT least(greatest(p_batch, 1), 5000)
  ),
  -- Those alone locked, as they were chosen. A record is changed only when
  -- what it names goes (its account erased, its run deleted: `user_id`,
  -- `run_id` emptied), its age never; one such a change holds is skipped,
  -- never waited on, so that neither waits for the other: the batch comes
  -- back short, and the next pass takes it.
  chosen AS (
    SELECT u.tenant_id, u.id
      FROM usage_records u
      JOIN candidates k ON k.tenant_id = u.tenant_id AND k.id = u.id
     FOR UPDATE OF u SKIP LOCKED
  ),
  deleted AS (
    DELETE FROM usage_records u
     USING chosen
     WHERE u.tenant_id = chosen.tenant_id AND u.id = chosen.id
    RETURNING u.tenant_id, u.supply_formula, u.billed_micros
  ),
  lost AS (
    SELECT d.tenant_id, count(*)::integer AS records
      FROM deleted d
     GROUP BY d.tenant_id
  ),
  -- What the prepaid calls purged consumed, kept for the balance.
  kept AS (
    INSERT INTO tenant_prepaid_purged AS p (tenant_id, billed_micros)
    SELECT d.tenant_id, sum(d.billed_micros)
      FROM deleted d
     WHERE d.supply_formula = 'prepaid'
     GROUP BY d.tenant_id
    ON CONFLICT (tenant_id) DO UPDATE
      SET billed_micros = p.billed_micros + excluded.billed_micros, updated_at = now()
    RETURNING 1
  ),
  audited AS (
    INSERT INTO audit_log (tenant_id, actor_type, action, outcome, metadata)
    SELECT lost.tenant_id, 'system', 'usage.purged', 'success',
           jsonb_build_object('records', lost.records)
      FROM lost
    RETURNING 1
  )
  SELECT lost.tenant_id, lost.records FROM lost
$$;

REVOKE ALL ON FUNCTION system_purge_old_usage(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_purge_old_usage(integer) TO mimorya_app;
