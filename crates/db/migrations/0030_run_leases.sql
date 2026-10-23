-- Execution leases (ADR-083): the process executing a run holds it under a
-- lease it renews while the run executes; a run whose lease has expired is
-- taken up by an executor. The initial schema reserved the columns (and
-- `agent_runs_queue_idx` on status and expiry); the owner becomes the
-- executor's identifier. A run that leaves `running` (paused, finished,
-- failed, swept) belongs to nobody: the trigger below gives its lease up,
-- whatever path changed its status.
ALTER TABLE agent_runs
  ALTER COLUMN lease_owner TYPE uuid USING lease_owner::uuid;

CREATE FUNCTION agent_runs_give_up_lease() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.status <> 'running' THEN
    NEW.lease_owner := NULL;
    NEW.lease_expires_at := NULL;
  END IF;
  RETURN NEW;
END
$$;

CREATE TRIGGER agent_runs_give_up_lease
  BEFORE INSERT OR UPDATE ON agent_runs
  FOR EACH ROW EXECUTE FUNCTION agent_runs_give_up_lease();

-- The runs of every company whose lease has expired, the longest expired
-- first: each is then taken up in its own company. Only their company and
-- identifier cross the companies' boundary. Leases are set and read on the
-- database's clock alone, whatever the executors' clocks say.
CREATE FUNCTION system_expired_run_leases()
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT agent_runs.tenant_id, agent_runs.id
    FROM agent_runs
   WHERE agent_runs.status = 'running'
     AND agent_runs.lease_expires_at < clock_timestamp()
   ORDER BY agent_runs.lease_expires_at
$$;

REVOKE ALL ON FUNCTION system_expired_run_leases() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_expired_run_leases() TO mimorya_app;
