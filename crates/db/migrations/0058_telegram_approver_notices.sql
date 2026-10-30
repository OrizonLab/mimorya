-- Requests awaiting a manager announced in Telegram too (ADR-137): the
-- manager, or whom they named to stand in for them, linked to the
-- company's Telegram bot, is written in their private chat with it, as in
-- Slack (ADR-130). A company may have both: each of its gateways tells of
-- a request once, what became of its notice kept for each.
ALTER TABLE gateway_approval_notices DROP CONSTRAINT gateway_approval_notices_pkey;
ALTER TABLE gateway_approval_notices ADD PRIMARY KEY (approval_id, gateway_id);

-- The paused runs, of every active company whose Slack application or
-- Telegram bot is enabled, with requests made since `p_since` awaiting a
-- manager whose account is active and whose notice through that gateway is
-- still to try at `p_now`: never tried, or due again. The same requests as
-- the read of each company (`gateway_approval_notices`), so that a run
-- picked is told of or recorded, never picked again for nothing. Their
-- company, their run and the gateway, the oldest request first, at most
-- `p_limit`. Nothing else crosses the companies' boundary: who is told,
-- and what, is read in each company.
DROP FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer);

CREATE FUNCTION system_gateway_approvals_to_notify(
  p_since timestamptz, p_now timestamptz, p_limit integer)
RETURNS TABLE (tenant_id uuid, run_id uuid, gateway_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT a.tenant_id, a.run_id, g.id
    FROM approvals a
    JOIN tenants t ON t.id = a.tenant_id AND t.status = 'active'
    JOIN gateways g ON g.tenant_id = a.tenant_id AND g.kind IN ('slack', 'telegram')
     AND g.enabled
    JOIN agent_runs r ON r.id = a.run_id AND r.tenant_id = a.tenant_id
    JOIN users m ON m.id = a.approver_user_id AND m.status = 'active'
   WHERE a.status = 'pending'
     AND a.routing = 'manager'
     AND a.created_at > p_since
     AND r.status = 'awaiting_approval'
     AND NOT EXISTS (
       SELECT 1 FROM gateway_approval_notices n
        WHERE n.approval_id = a.id
          AND n.gateway_id = g.id
          AND (n.outcome IS NOT NULL OR n.retry_after > p_now))
   GROUP BY a.tenant_id, a.run_id, g.id
   ORDER BY min(a.created_at)
   LIMIT p_limit
$$;

REVOKE ALL ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION system_gateway_approvals_to_notify(timestamptz, timestamptz, integer) TO mimorya_app;
