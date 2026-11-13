-- The account each person links at their company's standard mail
-- (ADR-205): their identifier, and their password sealed by the server's
-- master key for this company, connector and person alone before it
-- reaches the database, never shown, exported nor written to a log. Kept
-- only once the company's IMAP and SMTP servers accepted it. One per
-- connector and person. It goes with the connector, with the person's
-- account, when the person removes it, when the account is disabled, and
-- when the company moves one of its servers to another host: a password
-- never follows the mail to a server its person did not sign in to.
CREATE TABLE mail_accounts (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  connector_id uuid NOT NULL,
  user_id uuid NOT NULL,
  username text NOT NULL,
  password_ciphertext bytea NOT NULL,
  password_iv bytea NOT NULL,
  password_tag bytea NOT NULL,
  key_version integer NOT NULL,
  linked_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, connector_id, user_id),
  CONSTRAINT mail_accounts_connector_id_fkey FOREIGN KEY (tenant_id, connector_id)
    REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT mail_accounts_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT mail_accounts_username CHECK (char_length(username) BETWEEN 1 AND 320)
);

-- A person's accounts, listed in their account, exported, and taken away
-- when the account is disabled.
CREATE INDEX mail_accounts_user ON mail_accounts (tenant_id, user_id);

ALTER TABLE mail_accounts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON mail_accounts AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON mail_accounts TO mimorya_app;
