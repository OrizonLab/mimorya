-- How a company's memory grows by itself (ADR-076): whether a lesson is
-- drawn from each run that used tools and proposed in the personal memory
-- of whom it served. Without a row, or off, no run is reread: nothing more
-- is sent to the model than the run itself.
CREATE TABLE tenant_memory_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  capture_lessons boolean NOT NULL DEFAULT false,
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE tenant_memory_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_memory_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_memory_settings TO mimorya_app;
