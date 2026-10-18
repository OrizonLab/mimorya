-- Maintenance operations that must span tenants. They are narrowly scoped
-- SECURITY DEFINER functions instead of a privileged application connection.
CREATE OR REPLACE FUNCTION system_fail_stale_runs(p_before timestamptz)
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH updated AS (
    UPDATE agent_runs
       SET status = 'failed',
           error = 'Exécution interrompue (service redémarré ou bloqué)',
           finished_at = now(),
           updated_at = now()
     WHERE status = 'running'
       AND updated_at < p_before
    RETURNING 1
  )
  SELECT count(*)::integer FROM updated
$$;

REVOKE ALL ON FUNCTION system_fail_stale_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_fail_stale_runs(timestamptz) TO mimorya_app;
