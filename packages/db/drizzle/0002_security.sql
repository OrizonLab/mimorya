-- Runtime role used by the application. It is not the owner of the tables and
-- has no BYPASSRLS, so row-level security always applies to it. Login and
-- password are configured by the deployment (see infra/postgres/init.sql).
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'mimorya_app') THEN
    CREATE ROLE mimorya_app NOLOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
  END IF;
END
$$;
--> statement-breakpoint
GRANT USAGE ON SCHEMA public TO mimorya_app;
--> statement-breakpoint
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO mimorya_app;
--> statement-breakpoint
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO mimorya_app;
--> statement-breakpoint
-- The audit log is append-only for the application.
REVOKE UPDATE, DELETE, TRUNCATE ON audit_log FROM mimorya_app;
--> statement-breakpoint
-- Pre-tenant lookups. These are the only paths that read across tenants and
-- they return the minimum needed to establish the tenant context.
CREATE OR REPLACE FUNCTION auth_find_login(p_email text)
RETURNS TABLE (user_id uuid, tenant_id uuid, password_hash text, status text, tenant_status text)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT u.id, u.tenant_id, u.password_hash, u.status, t.status
  FROM users u
  JOIN tenants t ON t.id = u.tenant_id
  WHERE u.email = p_email::citext
$$;
--> statement-breakpoint
CREATE OR REPLACE FUNCTION auth_resolve_session(p_token_hash text)
RETURNS TABLE (session_id uuid, user_id uuid, tenant_id uuid, expires_at timestamptz, absolute_expires_at timestamptz)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT s.id, s.user_id, s.tenant_id, s.expires_at, s.absolute_expires_at
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
--> statement-breakpoint
REVOKE ALL ON FUNCTION auth_find_login(text) FROM PUBLIC;
--> statement-breakpoint
REVOKE ALL ON FUNCTION auth_resolve_session(text) FROM PUBLIC;
--> statement-breakpoint
GRANT EXECUTE ON FUNCTION auth_find_login(text) TO mimorya_app;
--> statement-breakpoint
GRANT EXECUTE ON FUNCTION auth_resolve_session(text) TO mimorya_app;
