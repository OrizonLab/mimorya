-- Requests a manager has not decided the day before their deadline
-- (ADR-062): every minute, the API reminds their manager once, in their own
-- company. Only the company and the run cross the companies' boundary.
CREATE FUNCTION system_approval_reminder_runs(p_now timestamptz, p_until timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
   WHERE status = 'pending'
     AND routing = 'manager'
     AND expires_at > p_now
     AND expires_at <= p_until
     AND NOT EXISTS (
           SELECT 1 FROM audit_log
            WHERE audit_log.run_id = approvals.run_id
              AND audit_log.action = 'approval.reminded'
              AND audit_log.resource_id = approvals.id::text)
$$;

REVOKE ALL ON FUNCTION system_approval_reminder_runs(timestamptz, timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_approval_reminder_runs(timestamptz, timestamptz) TO mimorya_app;
