-- Consumption kept five years, then purged (DATA_MODEL §5, ADR-197): the
-- records billing reads, deleted a batch at a time, the oldest of all the
-- companies first. Each company that lost records finds how many in its
-- audit log, written by the platform in the deletion's own statement. The
-- horizon is the platform's, not a company's: billing evidence is not
-- shortened by who it bills, nor kept past five years by anyone.

CREATE FUNCTION system_purge_old_usage(p_batch integer)
RETURNS TABLE (tenant_id uuid, records integer)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH candidates AS (
    -- Each company's oldest, through its own index (tenant, time), then
    -- the oldest of them all, a batch at most. Records are only ever
    -- added: none to lock against a writer.
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
  deleted AS (
    DELETE FROM usage_records u
     USING candidates k
     WHERE u.tenant_id = k.tenant_id AND u.id = k.id
    RETURNING u.tenant_id
  ),
  lost AS (
    SELECT d.tenant_id, count(*)::integer AS records
      FROM deleted d
     GROUP BY d.tenant_id
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
