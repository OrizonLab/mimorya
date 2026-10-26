-- Notices to an inbound hook's author (ADR-111). A run a delivery started
-- (`agent_runs.channel = 'hook'`, ADR-109), paused on requests its author
-- decides, is told to them by e-mail, as a scheduled task's run is
-- (ADR-082): nobody watches it. A stop of the service between that pause
-- and the e-mail loses the notice: the periodic catch-up tells the author,
-- in their own company, of those still pending in the paused run, in an
-- active company, not expired by `p_now`. Only the company and the run
-- cross the companies' boundary.
CREATE FUNCTION system_unnotified_hook_runs(p_now timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
    JOIN agent_runs ON agent_runs.id = approvals.run_id
     AND agent_runs.channel = 'hook'
     AND agent_runs.on_behalf_of_user_id = approvals.requested_for_user_id
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

REVOKE ALL ON FUNCTION system_unnotified_hook_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unnotified_hook_runs(timestamptz) TO mimorya_app;
