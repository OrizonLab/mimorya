-- Requests awaiting a manager announced in Slack (ADR-130): the manager,
-- or whom they named to stand in for them, linked to the company's Slack
-- application, receives a direct message of the requests of a paused run
-- awaiting them, with the buttons of those that may be validated there.

-- What became of the notice of a request in Slack: `sent` to `user_id`;
-- `refused` by Slack; `failed` after its attempts; `skipped` (gateway
-- paused, nobody linked to tell, Slack no longer saying the Slack user is
-- the person). Without an outcome, it is tried again after `retry_after`.
-- Only identifiers, never what was written: that is the request itself.
-- A notice goes with its request, its gateway and whom it was meant for.
CREATE TABLE gateway_approval_notices (
  approval_id uuid PRIMARY KEY REFERENCES approvals(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  gateway_id uuid NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  outcome text,
  attempts integer NOT NULL DEFAULT 1,
  retry_after timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gateway_approval_notices_outcome
    CHECK (outcome IN ('sent', 'refused', 'failed', 'skipped')),
  CONSTRAINT gateway_approval_notices_pending
    CHECK ((outcome IS NULL) = (retry_after IS NOT NULL))
);

CREATE INDEX gateway_approval_notices_gateway_idx ON gateway_approval_notices (gateway_id);
CREATE INDEX gateway_approval_notices_user_idx ON gateway_approval_notices (tenant_id, user_id);

ALTER TABLE gateway_approval_notices ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON gateway_approval_notices AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE ON gateway_approval_notices TO mimorya_app;

-- The pending requests a manager decides, read by the pass below.
CREATE INDEX approvals_pending_manager_idx ON approvals (created_at)
  WHERE status = 'pending' AND routing = 'manager';

-- The paused runs, of every active company whose Slack application is
-- enabled, with requests made since `p_since` awaiting a manager whose
-- account is active and whose notice in Slack is still to try at `p_now`:
-- never tried, or due again. The same requests as the read of each
-- company (`gateway_approval_notices`), so that a run picked is told of or
-- recorded, never picked again for nothing. Their company and their run,
-- the oldest request first, at most `p_limit`. Nothing else crosses the
-- companies' boundary: who is told, and what, is read in each company.
CREATE FUNCTION system_gateway_approvals_to_notify(
  p_since timestamptz, p_now timestamptz, p_limit integer)
RETURNS TABLE (tenant_id uuid, run_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT a.tenant_id, a.run_id
    FROM approvals a
    JOIN tenants t ON t.id = a.tenant_id AND t.status = 'active'
    JOIN gateways g ON g.tenant_id = a.tenant_id AND g.kind = 'slack' AND g.enabled
    JOIN agent_runs r ON r.id = a.run_id AND r.tenant_id = a.tenant_id
    JOIN users m ON m.id = a.approver_user_id AND m.status = 'active'
   WHERE a.status = 'pending'
     AND a.routing = 'manager'
     AND a.created_at > p_since
     AND r.status = 'awaiting_approval'
     AND NOT EXISTS (
       SELECT 1 FROM gateway_approval_notices n
        WHERE n.approval_id = a.id
          AND (n.outcome IS NOT NULL OR n.retry_after > p_now))
   GROUP BY a.tenant_id, a.run_id
   ORDER BY min(a.created_at)
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) TO mimorya_app;
