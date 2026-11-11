-- The audit log kept five years, its verification with it (DATA_MODEL §5,
-- ADR-198). A company's chain (ADR-178) loses its oldest places, a prefix
-- of it alone: from its first place, each five years old, each holding its
-- seal, the places following one another with no gap, never its last
-- sealed place. Where it then starts is kept (`audit_chain_starts`): its
-- first place, the seal of the place before it, the last one deleted, and
-- how many places went, sealed together under the chain's key, from which
-- the verification goes on as from the chain's own start. The horizon is
-- the platform's, as the consumption's (ADR-197): no company shortens its
-- own audit trail, nor keeps it longer.
--
-- The seals are checked, and the start sealed, by the process that holds
-- the master key (the retention pass): the database has no key. What it
-- checks again here is what it can: the places' age, that they follow one
-- another from the start, that the last sealed place stays, that the seal
-- kept is the one at the last place deleted. A start the database owner
-- writes, or moves, without the key holds no seal: the verification, the
-- sealing and the export take it for none (ADR-198).

CREATE TABLE audit_chain_starts (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  -- The chain's first place now.
  first_seq bigint NOT NULL,
  -- The seal of the place before it, the last one deleted.
  previous_hash bytea NOT NULL,
  -- How many places were deleted in all, and when last.
  purged_entries bigint NOT NULL,
  -- The start's own seal, under the chain's key: the company, the three
  -- above (ADR-198).
  seal bytea NOT NULL,
  purged_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT audit_chain_starts_check CHECK (
    first_seq > 1
    AND octet_length(previous_hash) = 32
    AND purged_entries > 0
    AND octet_length(seal) = 32
  )
);

ALTER TABLE audit_chain_starts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON audit_chain_starts AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- Read by the verification, the sealing, the purge and the export; written
-- by the purge's function alone.
GRANT SELECT ON audit_chain_starts TO mimorya_app;

-- A chain's start only moves forward.
CREATE FUNCTION audit_chain_starts_forward() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.first_seq <= OLD.first_seq OR NEW.purged_entries <= OLD.purged_entries THEN
    RAISE EXCEPTION 'audit chain: a chain''s start only moves forward';
  END IF;
  RETURN NEW;
END
$$;

CREATE TRIGGER audit_chain_starts_forward
  BEFORE UPDATE ON audit_chain_starts
  FOR EACH ROW EXECUTE FUNCTION audit_chain_starts_forward();

-- The companies whose chain's first place is five years old, the oldest
-- first: each is then read, checked and purged in its own company.
CREATE FUNCTION system_audit_purge_candidates()
RETURNS TABLE (tenant_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT c.id
    FROM (SELECT t.id,
                 (SELECT a.occurred_at FROM audit_log a
                   WHERE a.tenant_id = t.id
                     AND a.chain_seq = coalesce((SELECT s.first_seq FROM audit_chain_starts s
                                                  WHERE s.tenant_id = t.id), 1)
                   ORDER BY a.id LIMIT 1) AS first_at
            FROM tenants t) c
   WHERE c.first_at < now() - interval '5 years'
   ORDER BY c.first_at, c.id
$$;

REVOKE ALL ON FUNCTION system_audit_purge_candidates() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_audit_purge_candidates() TO mimorya_app;

-- Deletes the places `p_first` to `p_through` of the company `p_tenant`'s
-- chain, called in that company's transaction by the process that checked
-- their seals, and keeps where the chain then starts: `p_previous_hash`,
-- the seal at `p_through`, `p_purged_entries` places gone in all, and the
-- start's seal `p_seal`. Raises, and deletes nothing, unless `p_first` is
-- where the chain starts, the places follow one another to `p_through`,
-- each once, each five years old, 5000 at most, the last sealed place
-- after them, the seal at `p_through` the one given, and the count the
-- one before with these. The company finds the purge in its audit log
-- (`audit.purged`, sealed in turn); how many places went.
CREATE FUNCTION system_purge_old_audit_entries(p_tenant uuid, p_first bigint, p_through bigint,
                                               p_previous_hash bytea, p_purged_entries bigint,
                                               p_seal bytea)
RETURNS integer
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
  v_count bigint := p_through - p_first + 1;
  v_first bigint;
  v_purged bigint;
  v_head bigint;
  v_places bigint;
  v_distinct bigint;
  v_young bigint;
  v_hash bytea;
  v_deleted integer;
BEGIN
  IF p_tenant IS NULL OR p_first IS NULL OR p_through IS NULL OR v_count < 1 OR v_count > 5000 THEN
    RAISE EXCEPTION 'audit chain: % places asked, from 1 to 5000', v_count;
  END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('mimorya.audit_chain:' || p_tenant::text, 0));
  SELECT coalesce((SELECT s.first_seq FROM audit_chain_starts s WHERE s.tenant_id = p_tenant), 1),
         coalesce((SELECT s.purged_entries FROM audit_chain_starts s WHERE s.tenant_id = p_tenant), 0)
    INTO v_first, v_purged;
  IF p_first <> v_first THEN
    RAISE EXCEPTION 'audit chain: it starts at place %, not %', v_first, p_first;
  END IF;
  IF p_purged_entries IS DISTINCT FROM v_purged + v_count THEN
    RAISE EXCEPTION 'audit chain: % places gone in all, not %', v_purged + v_count, p_purged_entries;
  END IF;
  SELECT max(a.chain_seq) INTO v_head FROM audit_log a WHERE a.tenant_id = p_tenant;
  IF v_head IS NULL OR v_head <= p_through THEN
    RAISE EXCEPTION 'audit chain: its last sealed place stays';
  END IF;
  SELECT count(*), count(DISTINCT a.chain_seq),
         count(*) FILTER (WHERE a.occurred_at >= now() - interval '5 years')
    INTO v_places, v_distinct, v_young
    FROM audit_log a
   WHERE a.tenant_id = p_tenant AND a.chain_seq BETWEEN p_first AND p_through;
  IF v_places <> v_count OR v_distinct <> v_count THEN
    RAISE EXCEPTION 'audit chain: places % to % do not follow one another', p_first, p_through;
  END IF;
  IF v_young > 0 THEN
    RAISE EXCEPTION 'audit chain: % of these places are not five years old', v_young;
  END IF;
  SELECT a.chain_hash INTO v_hash
    FROM audit_log a WHERE a.tenant_id = p_tenant AND a.chain_seq = p_through;
  IF v_hash IS DISTINCT FROM p_previous_hash THEN
    RAISE EXCEPTION 'audit chain: the seal kept is not the one at place %', p_through;
  END IF;
  DELETE FROM audit_log a
   WHERE a.tenant_id = p_tenant AND a.chain_seq BETWEEN p_first AND p_through;
  GET DIAGNOSTICS v_deleted = ROW_COUNT;
  IF v_deleted <> v_count THEN
    RAISE EXCEPTION 'audit chain: % places read, % deleted', v_count, v_deleted;
  END IF;
  INSERT INTO audit_chain_starts AS s (tenant_id, first_seq, previous_hash, purged_entries, seal)
  VALUES (p_tenant, p_through + 1, p_previous_hash, p_purged_entries, p_seal)
  ON CONFLICT ON CONSTRAINT audit_chain_starts_pkey DO UPDATE
     SET first_seq = excluded.first_seq,
         previous_hash = excluded.previous_hash,
         purged_entries = excluded.purged_entries,
         seal = excluded.seal,
         purged_at = now();
  INSERT INTO audit_log (tenant_id, actor_type, action, outcome, metadata)
  VALUES (p_tenant, 'system', 'audit.purged', 'success',
          jsonb_build_object('entries', v_deleted, 'throughPlace', p_through));
  RETURN v_deleted;
END
$$;

REVOKE ALL ON FUNCTION system_purge_old_audit_entries(uuid, bigint, bigint, bytea, bigint, bytea)
  FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_old_audit_entries(uuid, bigint, bigint, bytea, bigint, bytea)
  TO mimorya_app;
