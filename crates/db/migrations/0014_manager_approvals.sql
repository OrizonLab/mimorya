-- Validation by a manager (ADR-059). A company names the risks whose actions
-- its managers validate; without a row, or with none named, the person the
-- agent acts for validates them, as before.
CREATE TABLE approval_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  manager_risks text[] NOT NULL DEFAULT '{}',
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT approval_settings_risks_check CHECK (
    manager_risks <@ ARRAY['read', 'draft', 'write', 'external', 'irreversible']::text[]
  )
);

ALTER TABLE approval_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON approval_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON approval_settings TO mimorya_app;

-- Who decides a request: the person the agent acts for (`requester`), or the
-- manager named when it was made (`manager`). A manager's account deleted
-- leaves nobody to decide it: it expires, never falls back to the requester.
ALTER TABLE approvals
  ADD COLUMN routing text NOT NULL DEFAULT 'requester',
  ADD COLUMN approver_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  ADD CONSTRAINT approvals_routing_check CHECK (routing IN ('requester', 'manager'));

CREATE INDEX approvals_approver_idx ON approvals (tenant_id, status, approver_user_id)
  WHERE approver_user_id IS NOT NULL;
