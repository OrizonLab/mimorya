-- Requests a manager was never told of (ADR-070): a stop of the service
-- between the pause of their run and the notice's e-mail loses it. At
-- startup, the API tells their manager, in their own company, of those
-- still pending in a paused run of an active company and due after
-- `p_due_after` (sooner ones are reminded instead, ADR-062). Only the
-- company and the run cross the companies' boundary.
CREATE FUNCTION system_unnotified_approval_runs(p_due_after timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
    JOIN agent_runs ON agent_runs.id = approvals.run_id
    JOIN tenants ON tenants.id = approvals.tenant_id
   WHERE approvals.status = 'pending'
     AND approvals.routing = 'manager'
     AND approvals.expires_at > p_due_after
     AND agent_runs.status = 'awaiting_approval'
     AND tenants.status = 'active'
     AND NOT EXISTS (
           SELECT 1 FROM audit_log
            WHERE audit_log.run_id = approvals.run_id
              AND audit_log.action = 'approval.notified'
              AND audit_log.resource_id = approvals.id::text)
$$;

REVOKE ALL ON FUNCTION system_unnotified_approval_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unnotified_approval_runs(timestamptz) TO mimorya_app;
