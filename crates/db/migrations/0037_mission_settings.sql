-- A company's own mission limits (ADR-093): how many stages a mission may
-- take and what it may cost, set by those who manage the company's
-- settings, within what the platform allows: one to twenty stages, 1 USD
-- to what twenty stages may cost (60 USD). Without a row, the platform's
-- (six stages, 12 USD). A mission keeps the limits it was entrusted with:
-- a change applies to the missions entrusted after it.
CREATE TABLE tenant_mission_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  max_stages integer NOT NULL,
  max_cost_micros bigint NOT NULL,
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_mission_settings_stages CHECK (max_stages BETWEEN 1 AND 20),
  CONSTRAINT tenant_mission_settings_cost
    CHECK (max_cost_micros BETWEEN 1000000 AND 60000000)
);

ALTER TABLE tenant_mission_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_mission_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_mission_settings TO mimorya_app;
