-- The other administrators of the access policies told by e-mail of the
-- allow policies proposed (ADR-170): every minute, the proposals still
-- waiting (pending, not expired) and never announced are told, in one
-- e-mail to each administrator of their company but the proposal's author.
-- `announced_at` says when, or that the relay refused it; a proposal
-- decided or expired before the pass is not announced, and one that nobody
-- but its author may decide waits for another administrator.
ALTER TABLE policy_proposals ADD COLUMN announced_at timestamptz;

CREATE INDEX policy_proposals_unannounced_idx ON policy_proposals (tenant_id)
  WHERE status = 'pending' AND announced_at IS NULL;

-- The active companies holding proposals still waiting and never
-- announced: each is then read in its own company.
CREATE FUNCTION system_unannounced_policy_proposals()
RETURNS TABLE (tenant_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT policy_proposals.tenant_id
    FROM policy_proposals
    JOIN tenants ON tenants.id = policy_proposals.tenant_id
   WHERE policy_proposals.status = 'pending'
     AND policy_proposals.expires_at > now()
     AND policy_proposals.announced_at IS NULL
     AND tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION system_unannounced_policy_proposals() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_unannounced_policy_proposals() TO mimorya_app;
