-- An account holding a temporary password (created or reset by an
-- administrator) must replace it before using the application.
ALTER TABLE users ADD COLUMN must_change_password boolean NOT NULL DEFAULT false;

-- Session resolution also says whether the password must be changed, so that
-- the API can hold every other request back without another query.
DROP FUNCTION auth_resolve_session(text);

CREATE FUNCTION auth_resolve_session(p_token_hash text)
RETURNS TABLE (
  session_id uuid,
  user_id uuid,
  tenant_id uuid,
  expires_at timestamptz,
  absolute_expires_at timestamptz,
  must_change_password boolean
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT s.id, s.user_id, s.tenant_id, s.expires_at, s.absolute_expires_at, u.must_change_password
  FROM sessions s
  JOIN users u ON u.id = s.user_id
  JOIN tenants t ON t.id = s.tenant_id
  WHERE s.token_hash = p_token_hash
    AND s.revoked_at IS NULL
    AND s.expires_at > now()
    AND s.absolute_expires_at > now()
    AND u.status = 'active'
    AND t.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_resolve_session(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_resolve_session(text) TO mimorya_app;
