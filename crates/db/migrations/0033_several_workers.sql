-- Several workers (ADR-086): no process alone executes runs any more, so a
-- run still running at startup may be another worker's. A run is taken up
-- once its lease has expired, or when it never had one (left running with
-- no executor: stopped before leases existed, or by hand), the latter
-- first. Each is then taken up in its own company, under its lock. Only
-- their company and identifier cross the companies' boundary.
CREATE OR REPLACE FUNCTION system_expired_run_leases()
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT agent_runs.tenant_id, agent_runs.id
    FROM agent_runs
   WHERE agent_runs.status = 'running'
     AND (agent_runs.lease_expires_at IS NULL
          OR agent_runs.lease_expires_at < clock_timestamp())
   ORDER BY agent_runs.lease_expires_at NULLS FIRST
$$;

-- Every run still running, which a single instance took up at startup
-- whatever its lease: replaced by the function above.
DROP FUNCTION system_interrupted_runs();
