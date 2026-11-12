-- What a company requires of its password accounts (ADR-201): a second
-- factor for its administrators, or for everyone. Without a row, no one
-- must have one: each person chooses. Set by those who manage its
-- settings; who set it last is kept, emptied with their account.
CREATE TABLE tenant_second_factor_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  requirement text NOT NULL,
  updated_by_user_id uuid,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_second_factor_settings_requirement_check
    CHECK (requirement IN ('optional', 'administrators', 'everyone')),
  CONSTRAINT tenant_second_factor_settings_updated_by_user_id_fkey
    FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id)
);

CREATE INDEX tenant_second_factor_settings_updated_by_person
  ON tenant_second_factor_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;

ALTER TABLE tenant_second_factor_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_second_factor_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_second_factor_settings TO mimorya_app;
