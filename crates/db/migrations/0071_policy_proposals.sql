-- Allow policies take two administrators (ADR-168): one proposes the policy
-- as it will stand, a new one or a change of an existing one; another
-- approves it, which writes it, or rejects it; its author may withdraw it.
-- A proposal waits until expires_at, past which nobody approves it. A change
-- is approved only while its policy is as it was when proposed
-- (policy_updated_at). Disabling or deleting an allow policy only takes
-- access away: it stays a direct change, and a policy deleted takes its
-- proposals with it. A proposal goes with its author's account; whoever
-- decided it is kept while their account is. Decided or expired, a proposal
-- is purged ninety days later (system_purge_policy_proposals); the audit log
-- keeps what was proposed and decided.

ALTER TABLE access_policies ADD CONSTRAINT access_policies_tenant_id_id_key UNIQUE (tenant_id, id);

CREATE TABLE policy_proposals (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  policy_id uuid,
  policy_updated_at timestamptz,
  description text NOT NULL,
  permissions text[] NOT NULL,
  condition jsonb NOT NULL,
  enabled boolean NOT NULL,
  proposed_by_user_id uuid NOT NULL,
  proposed_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  status text NOT NULL DEFAULT 'pending',
  decided_by_user_id uuid,
  decided_at timestamptz,
  CONSTRAINT policy_proposals_status_check
    CHECK (status IN ('pending', 'approved', 'rejected', 'withdrawn')),
  CONSTRAINT policy_proposals_decided CHECK ((status = 'pending') = (decided_at IS NULL)),
  CONSTRAINT policy_proposals_base CHECK ((policy_id IS NULL) = (policy_updated_at IS NULL)),
  CONSTRAINT policy_proposals_expiry CHECK (expires_at > proposed_at),
  CONSTRAINT policy_proposals_policy_id_fkey FOREIGN KEY (tenant_id, policy_id)
    REFERENCES access_policies (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT policy_proposals_proposed_by_user_id_fkey FOREIGN KEY (tenant_id, proposed_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT policy_proposals_decided_by_user_id_fkey FOREIGN KEY (tenant_id, decided_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (decided_by_user_id)
);

CREATE INDEX policy_proposals_tenant_idx ON policy_proposals (tenant_id, proposed_at);
CREATE INDEX policy_proposals_policy_idx ON policy_proposals (tenant_id, policy_id)
  WHERE policy_id IS NOT NULL;

ALTER TABLE policy_proposals ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON policy_proposals AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE ON policy_proposals TO mimorya_app;

-- Deletes the proposals of every company decided, or left to expire, before
-- `p_before`; how many.
CREATE FUNCTION system_purge_policy_proposals(p_before timestamptz)
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH purged AS (
    DELETE FROM policy_proposals
     WHERE decided_at < p_before OR (status = 'pending' AND expires_at < p_before)
    RETURNING 1
  )
  SELECT count(*)::integer FROM purged
$$;

REVOKE ALL ON FUNCTION system_purge_policy_proposals(timestamptz) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_policy_proposals(timestamptz) TO mimorya_app;
