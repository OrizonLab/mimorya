-- One-time invitation links (ADR-027): a new account chooses its own
-- password instead of receiving a temporary one. Only the SHA-256 digest of
-- the link's token is stored; an account has one pending invitation at most,
-- deleted once used.
CREATE TABLE user_invitations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  created_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL
);

CREATE INDEX user_invitations_tenant_idx ON user_invitations (tenant_id);

ALTER TABLE user_invitations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON user_invitations AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON user_invitations TO mimorya_app;

-- The only read before sign-in: the invitation a link designates, with
-- what its page shows (who is invited, by which company) and what decides
-- whether it can still be used.
CREATE FUNCTION auth_find_invitation(p_token_hash text)
RETURNS TABLE (
  invitation_id uuid,
  tenant_id uuid,
  user_id uuid,
  email text,
  display_name text,
  company text,
  expires_at timestamptz,
  user_status text,
  tenant_status text
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT i.id, i.tenant_id, i.user_id, u.email::text, u.display_name, t.name, i.expires_at,
         u.status, t.status
  FROM user_invitations i
  JOIN users u ON u.id = i.user_id AND u.tenant_id = i.tenant_id
  JOIN tenants t ON t.id = i.tenant_id
  WHERE i.token_hash = p_token_hash
$$;

REVOKE ALL ON FUNCTION auth_find_invitation(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_invitation(text) TO mimorya_app;
