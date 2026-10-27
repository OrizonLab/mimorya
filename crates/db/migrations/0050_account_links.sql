-- Accounts a person links at a provider their company declared (ADR-116):
-- their Microsoft 365 mailbox and calendar, reached with the rights they
-- granted themselves (delegated permissions), never more. The provider's
-- refresh token is sealed by the server's master key, bound to the link's
-- company, connector and person; it never leaves the server. What is kept
-- besides: who the provider says they are (its subject, the address read
-- in its token), the permissions granted, and when. A link goes with its
-- connector and its person; a disabled account loses its links.
CREATE TABLE account_links (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  connector_id uuid NOT NULL REFERENCES connector_instances(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  subject text NOT NULL,
  address text NOT NULL,
  scopes text NOT NULL,
  refresh_ciphertext bytea NOT NULL,
  refresh_iv bytea NOT NULL,
  refresh_auth_tag bytea NOT NULL,
  refresh_key_version integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  refreshed_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT account_links_one_per_person UNIQUE (connector_id, user_id),
  CONSTRAINT account_links_subject CHECK (char_length(subject) BETWEEN 1 AND 255),
  CONSTRAINT account_links_address CHECK (char_length(address) BETWEEN 3 AND 320),
  CONSTRAINT account_links_scopes CHECK (char_length(scopes) <= 1000)
);

CREATE INDEX account_links_user_idx ON account_links (tenant_id, user_id);

ALTER TABLE account_links ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON account_links AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON account_links TO mimorya_app;

-- One Microsoft 365 application per company: its people link their
-- mailbox there, and its tools carry fixed names.
CREATE UNIQUE INDEX connector_instances_one_microsoft365
  ON connector_instances (tenant_id) WHERE type = 'microsoft365';
