-- The OAuth authorization of an MCP server given by a person for themself
-- (ADR-174), when the administration declared that each person authorizes
-- it: the tokens its authorization server issued to that person, sealed by
-- the server's master key and bound to the connector and the person before
-- they reach the database, never shown. As for the company's authorization
-- (ADR-171): the access token with when it expires, the refresh token
-- apart, an access token that expires always renewable; the issuer, the
-- client and the resource they were issued for; the scopes granted; when
-- the person authorized and when the tokens were last renewed; a renewal
-- claimed by one process for a while, and why the last one failed. One
-- authorization per connector and person. It goes with the connector and
-- with the person's account; the platform removes it, and revokes its
-- tokens, when the person withdraws it, when the administration changes the
-- declaration it was given under, and when the account is disabled.
CREATE TABLE connector_person_authorizations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  connector_id uuid NOT NULL,
  user_id uuid NOT NULL,
  issuer text NOT NULL,
  client_id text NOT NULL,
  resource text NOT NULL,
  scopes text NOT NULL DEFAULT '',
  access_ciphertext bytea NOT NULL,
  access_iv bytea NOT NULL,
  access_auth_tag bytea NOT NULL,
  access_key_version integer NOT NULL,
  access_expires_at timestamptz,
  refresh_ciphertext bytea,
  refresh_iv bytea,
  refresh_auth_tag bytea,
  refresh_key_version integer,
  authorized_at timestamptz NOT NULL DEFAULT now(),
  refreshed_at timestamptz NOT NULL DEFAULT now(),
  renewing_until timestamptz,
  renewal_failure text CHECK (char_length(renewal_failure) BETWEEN 1 AND 300),
  CONSTRAINT connector_person_authorizations_one_per_person
    UNIQUE (tenant_id, connector_id, user_id),
  CONSTRAINT connector_person_authorizations_connector_id_fkey
    FOREIGN KEY (tenant_id, connector_id)
    REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT connector_person_authorizations_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE,
  -- The refresh token whole, or not at all.
  CONSTRAINT connector_person_authorizations_refresh_whole CHECK (
    num_nulls(refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version) IN (0, 4)
  ),
  -- An access token that expires can be renewed.
  CONSTRAINT connector_person_authorizations_renewable CHECK (
    access_expires_at IS NULL OR refresh_ciphertext IS NOT NULL
  ),
  CONSTRAINT connector_person_authorizations_issuer
    CHECK (char_length(issuer) BETWEEN 1 AND 2000),
  CONSTRAINT connector_person_authorizations_client
    CHECK (char_length(client_id) BETWEEN 1 AND 255),
  CONSTRAINT connector_person_authorizations_resource
    CHECK (char_length(resource) BETWEEN 1 AND 2000),
  CONSTRAINT connector_person_authorizations_scopes CHECK (char_length(scopes) <= 4100)
);

-- A person's authorizations, listed in their account and taken away when
-- the account is disabled.
CREATE INDEX connector_person_authorizations_user
  ON connector_person_authorizations (tenant_id, user_id);

ALTER TABLE connector_person_authorizations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON connector_person_authorizations AS PERMISSIVE FOR ALL
  TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON connector_person_authorizations TO mimorya_app;
