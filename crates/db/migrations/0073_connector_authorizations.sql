-- The OAuth authorization of an MCP server, given by a company-wide
-- connector administrator (ADR-171): the tokens its authorization server
-- issued for the company, sealed by the server's master key and bound to
-- the connector before they reach the database, never shown. The access
-- token is kept with when it expires; the refresh token apart, and an
-- access token that expires always has one. What is kept besides: the
-- issuer, the client and the resource the tokens are for, the scopes
-- granted, who authorized and when. One authorization per connector; it
-- goes with the connector, and the administration removes it with the
-- declaration it was given under.
CREATE TABLE connector_authorizations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  connector_id uuid NOT NULL,
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
  authorized_by_user_id uuid,
  authorized_at timestamptz NOT NULL DEFAULT now(),
  refreshed_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT connector_authorizations_one_per_connector UNIQUE (tenant_id, connector_id),
  CONSTRAINT connector_authorizations_connector_id_fkey FOREIGN KEY (tenant_id, connector_id)
    REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT connector_authorizations_authorized_by_user_id_fkey
    FOREIGN KEY (tenant_id, authorized_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (authorized_by_user_id),
  -- The refresh token whole, or not at all.
  CONSTRAINT connector_authorizations_refresh_whole CHECK (
    num_nulls(refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version) IN (0, 4)
  ),
  -- An access token that expires can be renewed.
  CONSTRAINT connector_authorizations_renewable CHECK (
    access_expires_at IS NULL OR refresh_ciphertext IS NOT NULL
  ),
  CONSTRAINT connector_authorizations_issuer CHECK (char_length(issuer) BETWEEN 1 AND 2000),
  CONSTRAINT connector_authorizations_client CHECK (char_length(client_id) BETWEEN 1 AND 255),
  CONSTRAINT connector_authorizations_resource CHECK (char_length(resource) BETWEEN 1 AND 2000),
  CONSTRAINT connector_authorizations_scopes CHECK (char_length(scopes) <= 4100)
);

ALTER TABLE connector_authorizations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON connector_authorizations AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON connector_authorizations TO mimorya_app;
