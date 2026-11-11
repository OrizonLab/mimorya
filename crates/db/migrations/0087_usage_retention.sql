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
