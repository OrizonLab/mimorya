-- The e-mails each person chooses to receive from the platform (ADR-094):
-- the requests awaiting their validation, as a manager or standing in for
-- one; the runs of their scheduled tasks and the stages of their missions
-- awaiting it; the end of their missions. Without a row, all of them.
-- Invitations and password resets are always sent. Deleted with the
-- account.
CREATE TABLE user_notification_settings (
  user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  approvals boolean NOT NULL DEFAULT true,
  own_runs boolean NOT NULL DEFAULT true,
  mission_ends boolean NOT NULL DEFAULT true,
  updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE user_notification_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON user_notification_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON user_notification_settings TO mimorya_app;
