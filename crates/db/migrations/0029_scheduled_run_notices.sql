-- Requests a scheduled task's author decides (ADR-082): its run started
-- while nobody watched (ADR-080), so the author is told by e-mail once it
-- pauses on them. A stop of the service between that pause and the e-mail
-- loses the notice: at startup, the API tells the author, in their own
-- company, of those still pending in the paused run their task last
-- started, in an active company, and not expired by `p_now` (one about to
-- expire is told of: authors have no reminder). Only the company and the
-- run cross the companies' boundary.
CREATE FUNCTION system_unnotified_scheduled_runs(p_now timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
    JOIN agent_runs ON agent_runs.id = approvals.run_id
    JOIN scheduled_tasks ON scheduled_tasks.last_run_id = approvals.run_id
     AND scheduled_tasks.user_id = approvals.requested_for_user_id
    JOIN tenants ON tenants.id = approvals.tenant_id
   WHERE approvals.status = 'pending'
     AND approvals.routing = 'requester'
     AND (approvals.expires_at IS NULL OR approvals.expires_at > p_now)
     AND agent_runs.status = 'awaiting_approval'
     AND tenants.status = 'active'
     AND NOT EXISTS (
           SELECT 1 FROM audit_log
            WHERE audit_log.run_id = approvals.run_id
              AND audit_log.action = 'approval.notified'
              AND audit_log.resource_id = approvals.id::text)
$$;

REVOKE ALL ON FUNCTION system_unnotified_scheduled_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unnotified_scheduled_runs(timestamptz) TO mimorya_app;
