-- What the evaluations of the company's skills may cost (ADR-154).
--
-- What they cost, day by day (UTC): each case played adds its cost as it
-- is recorded. Kept apart from the evaluations, which a skill prunes past
-- the last ones kept: pruning lowers nothing here. The rows of days long
-- past the window are deleted as new ones are written.
CREATE TABLE skill_evaluation_spending (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  day date NOT NULL,
  cost_micros bigint NOT NULL DEFAULT 0 CONSTRAINT skill_evaluation_spending_cost
    CHECK (cost_micros >= 0),
  PRIMARY KEY (tenant_id, day)
);

ALTER TABLE skill_evaluation_spending ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON skill_evaluation_spending AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON skill_evaluation_spending TO mimorya_app;

-- A company's own limit on what its evaluations may cost over thirty days,
-- set by those who manage its settings, from 1 USD to 1000 USD. Without a
-- row, the platform's (50 USD).
CREATE TABLE tenant_evaluation_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  max_cost_micros bigint NOT NULL,
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_evaluation_settings_cost
    CHECK (max_cost_micros BETWEEN 1000000 AND 1000000000)
);

ALTER TABLE tenant_evaluation_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_evaluation_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_evaluation_settings TO mimorya_app;
