-- Sub-agents (ADR-098): a run hands precise tasks to child runs, recorded
-- beside it in the columns the initial schema reserved (`parent_run_id`,
-- `agent_profile`). A child executes inside its parent's execution, under
-- its parent's lease, and holds none of its own: it is never taken up on
-- its own, and a child still running when its parent is taken up, fails,
-- is cancelled or ends fails with it.
ALTER TABLE agent_runs
  ADD CONSTRAINT agent_runs_profile_check
  CHECK (agent_profile IN ('main', 'research', 'analysis', 'planning', 'quality'));

-- A run's children, found without reading every run.
CREATE INDEX agent_runs_parent_idx ON agent_runs (parent_run_id)
  WHERE parent_run_id IS NOT NULL;

-- The runs of every company whose lease has expired or which never had
-- one, as before (ADR-086), children aside: their parent's lease is theirs.
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
     AND agent_runs.parent_run_id IS NULL
     AND (agent_runs.lease_expires_at IS NULL
          OR agent_runs.lease_expires_at < clock_timestamp())
   ORDER BY agent_runs.lease_expires_at NULLS FIRST
$$;

-- A run that ends, whatever path ended it (its engine, a crash, the sweep
-- of stuck runs, a mission stopped), ends its children still running.
CREATE FUNCTION agent_runs_end_sub_runs() RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.status IN ('completed', 'failed', 'cancelled')
     AND OLD.status NOT IN ('completed', 'failed', 'cancelled') THEN
    UPDATE agent_runs
       SET status = 'failed',
           error = 'Sous-agent interrompu : la demande qui l’avait lancé s’est arrêtée.',
           finished_at = now(),
           updated_at = now()
     WHERE parent_run_id = NEW.id
       AND status = 'running';
  END IF;
  RETURN NULL;
END
$$;

CREATE TRIGGER agent_runs_end_sub_runs
  AFTER UPDATE OF status ON agent_runs
  FOR EACH ROW EXECUTE FUNCTION agent_runs_end_sub_runs();
