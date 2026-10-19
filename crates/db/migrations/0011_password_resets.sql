-- Password reset links (ADR-028): an active account that forgot its password
-- asks for a one-time link, sent to its address. Only the SHA-256 digest of
-- the link's token is stored; an account has one pending link at most,
-- deleted once used.
CREATE TABLE password_resets (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL
);

CREATE INDEX password_resets_tenant_idx ON password_resets (tenant_id);

CREATE INDEX password_resets_expiry_idx ON password_resets (expires_at);

ALTER TABLE password_resets ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON password_resets AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON password_resets TO mimorya_app;

-- A pending link stops working as soon as the password changes by any other
-- path (the person's own change, an administrator's reset) or the account
-- stops being active, whichever code made the change. Runs as the caller,
-- under its tenant's row-level security.
CREATE FUNCTION retire_password_resets()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  DELETE FROM password_resets WHERE user_id = NEW.id;
  RETURN NULL;
END
$$;

CREATE TRIGGER users_retire_password_resets
  AFTER UPDATE OF password_hash, status ON users
  FOR EACH ROW
  WHEN (OLD.password_hash IS DISTINCT FROM NEW.password_hash OR NEW.status <> 'active')
  EXECUTE FUNCTION retire_password_resets();

-- The only read before sign-in: the reset a link designates, with the address
-- its page shows and what decides whether it can still be used.
CREATE FUNCTION auth_find_password_reset(p_token_hash text)
RETURNS TABLE (
  reset_id uuid,
  tenant_id uuid,
  user_id uuid,
  email text,
  expires_at timestamptz,
  user_status text,
  tenant_status text
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT r.id, r.tenant_id, r.user_id, u.email::text, r.expires_at, u.status, t.status
  FROM password_resets r
  JOIN users u ON u.id = r.user_id AND u.tenant_id = r.tenant_id
  JOIN tenants t ON t.id = r.tenant_id
  WHERE r.token_hash = p_token_hash
$$;

REVOKE ALL ON FUNCTION auth_find_password_reset(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_password_reset(text) TO mimorya_app;

-- Expired links of every company, deleted by the periodic maintenance: an
-- unused link leaves nothing behind but its audit entry.
CREATE FUNCTION system_purge_expired_password_resets()
RETURNS integer
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  WITH deleted AS (
    DELETE FROM password_resets WHERE expires_at <= now() RETURNING 1
  )
  SELECT count(*)::integer FROM deleted
$$;

REVOKE ALL ON FUNCTION system_purge_expired_password_resets() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_purge_expired_password_resets() TO mimorya_app;
