-- The run queue (ADR-085): a run an `api` process starts, or sets going
-- again after a decision, is queued (`queued`) for a worker, which claims
-- it in its company (`FOR UPDATE SKIP LOCKED`) and executes it under its
-- lease (ADR-083). The runs queued in every company, the longest waiting
-- first: only their company and identifier cross the companies' boundary.
CREATE FUNCTION system_queued_runs(p_limit integer)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT agent_runs.tenant_id, agent_runs.id
    FROM agent_runs
   WHERE agent_runs.status = 'queued'
   ORDER BY agent_runs.updated_at
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_queued_runs(integer) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_queued_runs(integer) TO mimorya_app;

-- The request of a turn (its user message): named on its run, a worker
-- gives it to the model with the conversation's earlier messages.
ALTER TABLE agent_runs
  ADD COLUMN request_message_id uuid REFERENCES messages (id) ON DELETE SET NULL;
