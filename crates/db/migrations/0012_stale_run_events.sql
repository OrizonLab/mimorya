-- Runs failed by the periodic sweep (crashed or stuck process) now tell the
-- clients following them: the function returns each run it fails, with its
-- error, for the API to record and broadcast the run's `run_finished` event.
DROP FUNCTION system_fail_stale_runs(timestamptz);

CREATE FUNCTION system_fail_stale_runs(p_before timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid, error text)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  UPDATE agent_runs
     SET status = 'failed',
         error = 'Exécution interrompue (service redémarré ou bloqué)',
         finished_at = now(),
         updated_at = now()
   WHERE status = 'running'
     AND updated_at < p_before
  RETURNING agent_runs.tenant_id, agent_runs.id, agent_runs.error
$$;

REVOKE ALL ON FUNCTION system_fail_stale_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_fail_stale_runs(timestamptz) TO mimorya_app;
