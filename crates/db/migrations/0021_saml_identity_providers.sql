-- SAML 2.0 identity providers (ADR-068), beside OpenID Connect ones: the
-- issuer is the provider's entity ID, the client the application's own
-- entity ID at the provider (the audience its assertions name), and the
-- provider's metadata (its sign-in address and signing certificates) is
-- kept as the company declared it. A SAML provider has no client secret.
ALTER TABLE identity_providers DROP CONSTRAINT identity_providers_kind_check;
ALTER TABLE identity_providers ADD CONSTRAINT identity_providers_kind_check
  CHECK (kind IN ('microsoft', 'google', 'oidc', 'saml'));

ALTER TABLE identity_providers ADD COLUMN saml_metadata text;
ALTER TABLE identity_providers ADD CONSTRAINT identity_providers_saml_metadata_check
  CHECK ((kind = 'saml') = (saml_metadata IS NOT NULL));

-- The assertions a SAML provider's returns carried, until they expire: one
-- is accepted once, whichever server receives it.
CREATE TABLE sso_used_assertions (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  provider_id uuid NOT NULL,
  assertion_id text NOT NULL,
  expires_at timestamptz NOT NULL,
  PRIMARY KEY (provider_id, assertion_id),
  CONSTRAINT sso_used_assertions_provider_fkey FOREIGN KEY (tenant_id, provider_id)
    REFERENCES identity_providers (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX sso_used_assertions_expiry_idx ON sso_used_assertions (provider_id, expires_at);

ALTER TABLE sso_used_assertions ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON sso_used_assertions AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, DELETE ON sso_used_assertions TO mimorya_app;
