-- The audit log kept five years, its verification with it (DATA_MODEL §5,
-- ADR-198). A company's chain (ADR-178) loses its oldest places, a prefix
-- of it alone: from its first place, each sealed, each five years old, the
-- places following one another with no gap, never its last sealed place.
-- Where it then starts is kept (`audit_chain_starts`): its first place and
-- the seal of the place before it, the last one deleted, from which the
-- verification goes on as from the chain's own start. The horizon is the
-- platform's, as the consumption's (ADR-197): no company shortens its own
-- audit trail, nor keeps it longer.
--
-- What the database cannot tell about the places deleted (an entry altered
-- before they went) goes with them: the verification of the chain, which
-- an administrator or the operator runs (ADR-179, ADR-180), tells it while
-- they are kept. A gap in them (an entry removed) stops the purge before
-- it, so that the verification still shows it.

CREATE TABLE audit_chain_starts (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  -- The chain's first place now.
  first_seq bigint NOT NULL,
  -- The seal of the place before it, the last one deleted.
  previous_hash bytea NOT NULL,
  -- How many places were deleted in all, and when last.
  purged_entries bigint NOT NULL,
  purged_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT audit_chain_starts_check CHECK (
    first_seq > 1
    AND octet_length(previous_hash) = 32
    AND purged_entries > 0
  )
);

ALTER TABLE audit_chain_starts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON audit_chain_starts AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- Read by the verification, the sealing and the export; written by the
-- purge alone.
GRANT SELECT ON audit_chain_starts TO mimorya_app;

-- A chain's start only moves forward.
CREATE FUNCTION audit_chain_starts_forward() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.first_seq <= OLD.first_seq OR NEW.purged_entries < OLD.purged_entries THEN
    RAISE EXCEPTION 'audit chain: a chain''s start only moves forward';
  END IF;
  RETURN NEW;
END
$$;

CREATE TRIGGER audit_chain_starts_forward
  BEFORE UPDATE ON audit_chain_starts
  FOR EACH ROW EXECUTE FUNCTION audit_chain_starts_forward();

-- Deletes, in every company, the oldest places of its chain five years
-- old, `p_batch` at most in all (the database keeps it from 1 to 5000),
-- the companies whose chain starts with the oldest entry first. Each
-- company is held as its sealing holds it, so that neither crosses the
-- other; each that lost places finds how many, and through which place, in
-- its audit log (`audit.purged`, sealed in turn). What each lost.
CREATE FUNCTION system_purge_old_audit_entries(p_batch integer)
RETURNS TABLE (tenant_id uuid, entries integer, through_seq bigint)
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
  v_left integer := least(greatest(coalesce(p_batch, 1), 1), 5000);
  v_horizon timestamptz := now() - interval '5 years';
  v_company uuid;
  v_first bigint;
  v_head bigint;
  v_through bigint;
  v_count integer;
  v_deleted integer;
  v_hash bytea;
BEGIN
  FOR v_company IN
    SELECT c.id
      FROM (SELECT t.id,
                   (SELECT a.occurred_at FROM audit_log a
                     WHERE a.tenant_id = t.id
                       AND a.chain_seq = coalesce((SELECT s.first_seq FROM audit_chain_starts s
                                                    WHERE s.tenant_id = t.id), 1)) AS first_at
              FROM tenants t) c
     WHERE c.first_at < v_horizon
     ORDER BY c.first_at
  LOOP
    EXIT WHEN v_left <= 0;
    PERFORM pg_advisory_xact_lock(hashtextextended('mimorya.audit_chain:' || v_company::text, 0));
    -- Read again under the lock.
    SELECT coalesce((SELECT s.first_seq FROM audit_chain_starts s WHERE s.tenant_id = v_company), 1)
      INTO v_first;
    SELECT max(a.chain_seq) INTO v_head FROM audit_log a WHERE a.tenant_id = v_company;
    -- The prefix: from the first place, in the chain's order, while each is
    -- five years old and at the place that follows, the last sealed one
    -- excepted; read a batch at most.
    WITH places AS (
      SELECT p.chain_seq, p.occurred_at, row_number() OVER (ORDER BY p.chain_seq) AS rn
        FROM (SELECT a.chain_seq, a.occurred_at
                FROM audit_log a
               WHERE a.tenant_id = v_company
                 AND a.chain_seq >= v_first
                 AND a.chain_seq < v_head
               ORDER BY a.chain_seq
               LIMIT v_left) p
    ),
    prefix AS (
      SELECT places.chain_seq,
             bool_and(places.occurred_at < v_horizon AND places.chain_seq = v_first + places.rn - 1)
               OVER (ORDER BY places.chain_seq) AS old
        FROM places
    )
    SELECT max(prefix.chain_seq), count(*)::integer INTO v_through, v_count
      FROM prefix WHERE prefix.old;
    CONTINUE WHEN v_count = 0;
    SELECT a.chain_hash INTO v_hash
      FROM audit_log a WHERE a.tenant_id = v_company AND a.chain_seq = v_through;
    DELETE FROM audit_log a
     WHERE a.tenant_id = v_company AND a.chain_seq >= v_first AND a.chain_seq <= v_through;
    GET DIAGNOSTICS v_deleted = ROW_COUNT;
    IF v_deleted <> v_count THEN
      RAISE EXCEPTION 'audit chain: % places read, % deleted', v_count, v_deleted;
    END IF;
    INSERT INTO audit_chain_starts AS s (tenant_id, first_seq, previous_hash, purged_entries)
    VALUES (v_company, v_through + 1, v_hash, v_count)
    ON CONFLICT ON CONSTRAINT audit_chain_starts_pkey DO UPDATE
       SET first_seq = excluded.first_seq,
           previous_hash = excluded.previous_hash,
           purged_entries = s.purged_entries + excluded.purged_entries,
           purged_at = now();
    INSERT INTO audit_log (tenant_id, actor_type, action, outcome, metadata)
    VALUES (v_company, 'system', 'audit.purged', 'success',
            jsonb_build_object('entries', v_count, 'throughPlace', v_through));
    tenant_id := v_company;
    entries := v_count;
    through_seq := v_through;
    RETURN NEXT;
    v_left := v_left - v_count;
  END LOOP;
END
$$;

REVOKE ALL ON FUNCTION system_purge_old_audit_entries(integer) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_old_audit_entries(integer) TO mimorya_app;
