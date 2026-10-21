-- Sign-in through a company's identity provider (ADR-063): OpenID Connect,
-- Microsoft 365 (Entra ID) first. A company declares its providers and the
-- e-mail domains each one covers; an account is tied to its identity at a
-- provider at its first sign-in there, never created by it.
CREATE TABLE identity_providers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  kind text NOT NULL,
  name text NOT NULL,
  -- Fixed once declared, with the client: the identities tied to the
  -- provider are this issuer's subjects for this client.
  issuer text NOT NULL,
  client_id text NOT NULL,
  enabled boolean NOT NULL DEFAULT true,
  created_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT identity_providers_kind_check CHECK (kind IN ('microsoft', 'google', 'oidc')),
  CONSTRAINT identity_providers_client_key UNIQUE (tenant_id, issuer, client_id),
  -- What the tables below reference: a row only ever points to a provider
  -- of its own company.
  CONSTRAINT identity_providers_tenant_key UNIQUE (tenant_id, id)
);

-- The e-mail domains a provider covers: one provider per domain within the
-- company. Another company may declare the same domain: a provider only
-- ever serves its own company's accounts.
CREATE TABLE identity_provider_domains (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  domain citext NOT NULL,
  provider_id uuid NOT NULL,
  PRIMARY KEY (tenant_id, domain),
  CONSTRAINT identity_provider_domains_provider_fkey FOREIGN KEY (tenant_id, provider_id)
    REFERENCES identity_providers (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX identity_provider_domains_provider_idx ON identity_provider_domains (provider_id);

-- An account's identity at a provider: its immutable subject there. One per
-- account and provider, one account per subject.
CREATE TABLE user_identities (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  provider_id uuid NOT NULL,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  subject text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT user_identities_provider_fkey FOREIGN KEY (tenant_id, provider_id)
    REFERENCES identity_providers (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT user_identities_subject_key UNIQUE (provider_id, subject),
  CONSTRAINT user_identities_user_key UNIQUE (provider_id, user_id)
);

CREATE INDEX user_identities_user_idx ON user_identities (user_id);

ALTER TABLE identity_providers ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity_provider_domains ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_identities ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON identity_providers AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

CREATE POLICY tenant_isolation ON identity_provider_domains AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

CREATE POLICY tenant_isolation ON user_identities AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity_providers TO mimorya_app;
GRANT SELECT, INSERT, DELETE ON identity_provider_domains TO mimorya_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON user_identities TO mimorya_app;

-- Before sign-in, the company is not known yet: the provider of the active
-- account with this address, if its company is active and has an enabled
-- provider covering the address's domain. Only what the sign-in page
-- shows, and where to continue, crosses the companies' boundary; a disabled
-- or invited account reads as none (an invitation is accepted first).
CREATE FUNCTION auth_find_identity_provider(p_email text)
RETURNS TABLE (tenant_id uuid, provider_id uuid, kind text, name text)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT p.tenant_id, p.id, p.kind, p.name
    FROM users u
    JOIN tenants t ON t.id = u.tenant_id
    JOIN identity_provider_domains d
      ON d.tenant_id = u.tenant_id AND d.domain = split_part(u.email::text, '@', 2)::citext
    JOIN identity_providers p ON p.id = d.provider_id AND p.tenant_id = d.tenant_id
   WHERE u.email = p_email::citext
     AND u.status = 'active'
     AND p.enabled
     AND t.status = 'active'
$$;

REVOKE ALL ON FUNCTION auth_find_identity_provider(text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION auth_find_identity_provider(text) TO mimorya_app;
