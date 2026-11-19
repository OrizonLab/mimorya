-- The alerts of the operator's supply (ADR-211): the administrators of a
-- company on the operator's key are told by e-mail when its month comes to
-- a share of its cap, or its credit's balance falls under a share of its
-- last payment. The operator alone sets the shares, with the owner role,
-- through `mimorya-admin`; by default 80 and 95 % of the cap, 20 and 5 % of
-- the last payment. Each alert is sent once for its month or its payment,
-- whichever process runs the pass.
ALTER TABLE tenant_model_supply
  -- Shares of the monthly cap, in percent, from 1 to 100 (the cap reached).
  -- One dimension, from its first element: as the application reads it.
  ADD COLUMN cap_alert_percents smallint[] NOT NULL DEFAULT '{80,95}'
    CONSTRAINT tenant_model_supply_cap_alerts CHECK (
      cardinality(cap_alert_percents) <= 5
      AND (cardinality(cap_alert_percents) = 0
           OR (array_ndims(cap_alert_percents) = 1 AND array_lower(cap_alert_percents, 1) = 1))
      AND array_position(cap_alert_percents, NULL) IS NULL
      AND 1 <= ALL (cap_alert_percents) AND 100 >= ALL (cap_alert_percents)
    ),
  -- Shares of the last payment, in percent, from 0 (the credit spent) to 99.
  ADD COLUMN credit_alert_percents smallint[] NOT NULL DEFAULT '{5,20}'
    CONSTRAINT tenant_model_supply_credit_alerts CHECK (
      cardinality(credit_alert_percents) <= 5
      AND (cardinality(credit_alert_percents) = 0
           OR (array_ndims(credit_alert_percents) = 1
               AND array_lower(credit_alert_percents, 1) = 1))
      AND array_position(credit_alert_percents, NULL) IS NULL
      AND 0 <= ALL (credit_alert_percents) AND 99 >= ALL (credit_alert_percents)
    );

-- The alerts sent: one per share, for a month of the cap and that cap
-- (`period`, `YYYY-MM:` and the cap in micro-dollars, re-armed when the cap is
-- raised) or a payment of the credit (`period`, its reference). The
-- application records them as it sends them; none is ever changed.
CREATE TABLE tenant_supply_alerts (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  kind text NOT NULL
    CONSTRAINT tenant_supply_alerts_kind CHECK (kind IN ('cap', 'credit')),
  period text NOT NULL
    CONSTRAINT tenant_supply_alerts_period CHECK (char_length(period) BETWEEN 1 AND 100),
  percent smallint NOT NULL
    CONSTRAINT tenant_supply_alerts_percent CHECK (percent BETWEEN 0 AND 100),
  sent_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, kind, period, percent)
);

ALTER TABLE tenant_supply_alerts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_supply_alerts AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT ON tenant_supply_alerts TO mimorya_app;

-- The active companies on the operator's key, whose supply the pass reads,
-- each in its own company.
CREATE FUNCTION system_supplied_companies()
RETURNS TABLE (tenant_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT tenant_model_supply.tenant_id
    FROM tenant_model_supply
    JOIN tenants ON tenants.id = tenant_model_supply.tenant_id
   WHERE tenant_model_supply.formula IN ('capped', 'prepaid')
     AND tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION system_supplied_companies() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_supplied_companies() TO mimorya_app;
