-- Requests nobody decided before their deadline (ADR-061). Every minute, the
-- API expires them in their own company and lets their run go on. Only the
-- company and the run cross the companies' boundary.
CREATE FUNCTION system_overdue_approval_runs(p_now timestamptz)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT approvals.tenant_id, approvals.run_id
    FROM approvals
   WHERE status = 'pending'
     AND expires_at <= p_now
$$;

REVOKE ALL ON FUNCTION system_overdue_approval_runs(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_overdue_approval_runs(timestamptz) TO mimorya_app;

-- What the sweep looks for: few pending requests among all.
CREATE INDEX approvals_pending_expiry_idx ON approvals (expires_at) WHERE status = 'pending';
