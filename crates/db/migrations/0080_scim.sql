-- SCIM 2.0 provisioning of a company's accounts by its identity provider
-- (ADR-183).

-- The tokens with which the company's identity provider provisions its
-- accounts: each named by the administrator who made it, shown once when
-- made, only its digest kept, never logged nor audited; when it was last
-- used. A token reaches its company's accounts through SCIM, and nothing
-- else.
CREATE TABLE scim_tokens (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
  token_hash text NOT NULL CHECK (char_length(token_hash) = 64),
  created_by uuid,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz,
  CONSTRAINT scim_tokens_token_hash_key UNIQUE (token_hash),
  CONSTRAINT scim_tokens_created_by_fkey FOREIGN KEY (tenant_id, created_by)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by)
);

CREATE INDEX scim_tokens_tenant ON scim_tokens (tenant_id, created_at);

ALTER TABLE scim_tokens ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON scim_tokens AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- A token is made and removed whole; only when it was last used changes.
GRANT SELECT, INSERT, DELETE ON scim_tokens TO mimorya_app;
GRANT UPDATE (last_used_at) ON scim_tokens TO mimorya_app;

-- The company and the token whose digest is `token_hash`, before any
-- session, if that company is active: an identity provider proves itself by
-- its token alone. Nothing else crosses the companies' boundary; an unknown
-- token and a suspended company's answer alike.
CREATE FUNCTION auth_scim_token(token_hash text)
RETURNS TABLE (tenant_id uuid, token_id uuid)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT scim_tokens.tenant_id, scim_tokens.id
    FROM scim_tokens
    JOIN tenants ON tenants.id = scim_tokens.tenant_id
   WHERE scim_tokens.token_hash = auth_scim_token.token_hash
     AND tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_scim_token(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_scim_token(text) TO mimorya_app;

-- The identifier the company's identity provider gives an account
-- (`externalId`), when it gives one: one account per identifier in a
-- company. Gone with the account.
CREATE TABLE scim_accounts (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id uuid NOT NULL,
  external_id text NOT NULL CHECK (char_length(external_id) BETWEEN 1 AND 255),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT scim_accounts_user_key UNIQUE (tenant_id, user_id),
  CONSTRAINT scim_accounts_external_id_key UNIQUE (tenant_id, external_id),
  CONSTRAINT scim_accounts_user_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE scim_accounts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON scim_accounts AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON scim_accounts TO mimorya_app;
