-- Runs a stop of the service left running (ADR-053).
--
-- Each time a run's state is saved, the last event of its journal at that
-- moment is kept with it: the calls started since then (later events) are
-- those a stop may have left half done. A sequence number, unlike a clock,
-- never goes back.
ALTER TABLE agent_runs ADD COLUMN checkpoint_seq integer;

-- At startup, before it serves, the API takes each run still running up
-- again in its own company. Only their company and identifier cross the
-- companies' boundary.
CREATE FUNCTION system_interrupted_runs()
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT agent_runs.tenant_id, agent_runs.id
    FROM agent_runs
   WHERE status = 'running'
   ORDER BY updated_at
$$;

REVOKE ALL ON FUNCTION system_interrupted_runs() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_interrupted_runs() TO mimorya_app;
