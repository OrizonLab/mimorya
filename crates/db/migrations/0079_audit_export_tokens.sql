-- The tokens with which a company's SIEM reads its audit log (ADR-181):
-- each named by the administrator who made it, shown once when made, only
-- its digest kept, never logged nor audited; when it was last used. A token
-- reads its company's sealed entries in the chain's order, and nothing else.
CREATE TABLE audit_export_tokens (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
  token_hash text NOT NULL CHECK (char_length(token_hash) = 64),
  created_by uuid,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz,
  CONSTRAINT audit_export_tokens_token_hash_key UNIQUE (token_hash),
  CONSTRAINT audit_export_tokens_created_by_fkey FOREIGN KEY (tenant_id, created_by)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by)
);

CREATE INDEX audit_export_tokens_tenant ON audit_export_tokens (tenant_id, created_at);

ALTER TABLE audit_export_tokens ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON audit_export_tokens AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- A token is made and removed whole; only when it was last used changes.
GRANT SELECT, INSERT, DELETE ON audit_export_tokens TO mimorya_app;
GRANT UPDATE (last_used_at) ON audit_export_tokens TO mimorya_app;

-- The company and the token whose digest is `token_hash`, before any
-- session, if that company is active: a SIEM proves itself by its token
-- alone. Nothing else crosses the companies' boundary; an unknown token and
-- a suspended company's answer alike.
CREATE FUNCTION auth_audit_export_token(token_hash text)
RETURNS TABLE (tenant_id uuid, token_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT audit_export_tokens.tenant_id, audit_export_tokens.id
    FROM audit_export_tokens
    JOIN tenants ON tenants.id = audit_export_tokens.tenant_id
   WHERE audit_export_tokens.token_hash = auth_audit_export_token.token_hash
     AND tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_audit_export_token(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_audit_export_token(text) TO mimorya_app;

-- The platform's administrator role of the companies already there gains
-- what the new ones are created with: exporting the audit log.
UPDATE roles SET permissions = permissions || ARRAY['audit:export']
  WHERE is_system AND key = 'tenant_admin'
    AND NOT 'audit:export' = ANY (permissions);
