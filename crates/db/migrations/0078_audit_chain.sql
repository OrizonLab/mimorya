-- The audit log chained by company (ADR-178): each entry is given, after
-- the fact, its place in its company's chain (`chain_seq`, from 1, with no
-- gap) and a seal (`chain_hash`): the HMAC, under a key derived from the
-- server's master key, of the previous seal, its place and its content. An
-- entry altered or removed after its seal breaks the chain where it was;
-- only who holds the master key computes a seal that holds. What the
-- database alone cannot tell (seals taken away, then set again by the
-- sealing on altered entries; entries removed from the chain's end), the
-- seals logged outside it tell (ADR-178).
--
-- The application writes entries but never a seal directly: it may insert
-- only the columns of an entry's content, listed below (a column added to
-- the audit log later must be granted here too), and still may not update
-- nor delete. A seal is set by `system_seal_audit_entries` alone, once, in
-- order after its company's last one; a false one set so is not prevented,
-- it is found by the verification. Once set, a seal is never changed nor
-- taken away: the owner of the database must drop that rule first.

ALTER TABLE audit_log
  ADD COLUMN chain_seq bigint,
  ADD COLUMN chain_hash bytea,
  ADD CONSTRAINT audit_log_chain_check CHECK (
    (chain_seq IS NULL) = (chain_hash IS NULL)
    AND (chain_seq IS NULL OR chain_seq > 0)
    AND (chain_hash IS NULL OR octet_length(chain_hash) = 32)
  );

-- One entry per place in a company's chain, read in its order.
CREATE UNIQUE INDEX audit_log_chain_idx ON audit_log (tenant_id, chain_seq);

-- The entries still to seal, in the order they were written.
CREATE INDEX audit_log_unsealed_idx ON audit_log (tenant_id, id) WHERE chain_seq IS NULL;

REVOKE INSERT ON audit_log FROM mimorya_app;
GRANT INSERT (tenant_id, occurred_at, actor_type, actor_user_id, on_behalf_of_user_id, run_id,
              action, resource_type, resource_id, outcome, reason, metadata, ip, request_id)
  ON audit_log TO mimorya_app;

-- A seal, once set, is never changed nor taken away.
CREATE FUNCTION audit_log_keep_seals() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF OLD.chain_seq IS NOT NULL
     AND (NEW.chain_seq IS DISTINCT FROM OLD.chain_seq
          OR NEW.chain_hash IS DISTINCT FROM OLD.chain_hash) THEN
    RAISE EXCEPTION 'audit chain: a seal is never changed nor taken away';
  END IF;
  RETURN NEW;
END
$$;

CREATE TRIGGER audit_log_keep_seals
  BEFORE UPDATE OF chain_seq, chain_hash ON audit_log
  FOR EACH ROW EXECUTE FUNCTION audit_log_keep_seals();

-- The companies whose audit log has entries: with `p_unsealed`, only those
-- with entries still to seal (each query its own, so that the index of the
-- entries still to seal serves). Only their identifier crosses the
-- companies' boundary.
CREATE FUNCTION system_audit_log_tenants(p_unsealed boolean)
RETURNS TABLE (tenant_id uuid)
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
  IF p_unsealed THEN
    RETURN QUERY
      SELECT tenants.id FROM tenants
       WHERE EXISTS (SELECT 1 FROM audit_log
                      WHERE audit_log.tenant_id = tenants.id AND audit_log.chain_seq IS NULL)
       ORDER BY tenants.id;
  ELSE
    RETURN QUERY
      SELECT tenants.id FROM tenants
       WHERE EXISTS (SELECT 1 FROM audit_log WHERE audit_log.tenant_id = tenants.id)
       ORDER BY tenants.id;
  END IF;
END
$$;

REVOKE ALL ON FUNCTION system_audit_log_tenants(boolean) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_audit_log_tenants(boolean) TO mimorya_app;

-- Seals the entries `p_ids` of the company `p_tenant`, called in that
-- company's transaction, at the places `p_seqs` with the seals `p_hashes`,
-- in that order: each place follows the company's last one, and each entry
-- is an unsealed one of the company. Anything else raises, and nothing is
-- sealed. One sealing at a time per company: the seals were computed after
-- its last place, read under the same lock.
CREATE FUNCTION system_seal_audit_entries(p_tenant uuid, p_ids bigint[], p_seqs bigint[],
                                          p_hashes bytea[])
RETURNS integer
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
  v_count integer := coalesce(array_length(p_ids, 1), 0);
  v_head bigint;
BEGIN
  IF p_tenant IS DISTINCT FROM nullif(current_setting('app.tenant_id', true), '')::uuid THEN
    RAISE EXCEPTION 'audit chain: sealed in its own company''s transaction only';
  END IF;
  IF coalesce(array_length(p_seqs, 1), 0) <> v_count
     OR coalesce(array_length(p_hashes, 1), 0) <> v_count THEN
    RAISE EXCEPTION 'audit chain: as many places and seals as entries expected';
  END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('mimorya.audit_chain:' || p_tenant::text, 0));
  SELECT coalesce(max(chain_seq), 0) INTO v_head FROM audit_log WHERE tenant_id = p_tenant;
  FOR i IN 1 .. v_count LOOP
    IF p_seqs[i] IS DISTINCT FROM v_head + i THEN
      RAISE EXCEPTION 'audit chain: place % does not follow %', p_seqs[i], v_head + i - 1;
    END IF;
    UPDATE audit_log SET chain_seq = p_seqs[i], chain_hash = p_hashes[i]
     WHERE id = p_ids[i] AND tenant_id = p_tenant AND chain_seq IS NULL;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'audit chain: entry % is not an unsealed entry of the company', p_ids[i];
    END IF;
  END LOOP;
  RETURN v_count;
END
$$;

REVOKE ALL ON FUNCTION system_seal_audit_entries(uuid, bigint[], bigint[], bytea[]) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_seal_audit_entries(uuid, bigint[], bigint[], bytea[])
  TO mimorya_app;
