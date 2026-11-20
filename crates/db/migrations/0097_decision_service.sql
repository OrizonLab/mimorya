-- The decision service (ADR-213): quick, typed judgements asked of Jev or
-- of the company's main model.
--
-- One row per company that chose its engine, by those who manage its
-- settings: the main model (by default, without a row) or Jev, whose key
-- is sealed among the company's secrets.
CREATE TABLE tenant_decision_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  engine text NOT NULL,
  updated_by_user_id uuid,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_decision_settings_engine CHECK (engine IN ('main_model', 'jev')),
  CONSTRAINT tenant_decision_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id)
);

ALTER TABLE tenant_decision_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_decision_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_decision_settings TO mimorya_app;

-- An erased account empties the rows it last changed (ADR-189).
CREATE INDEX tenant_decision_settings_updated_by_person
  ON tenant_decision_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;

-- Each usage of a company, set off (as without a row), observed (decided
-- and journaled, without effect) or active, with its threshold.
CREATE TABLE tenant_decision_usages (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  usage text NOT NULL,
  mode text NOT NULL,
  threshold double precision NOT NULL,
  PRIMARY KEY (tenant_id, usage),
  CONSTRAINT tenant_decision_usages_usage CHECK (usage IN (
    'model_routing', 'skill_choice', 'memory_guard', 'document_labels', 'action_guard',
    'injection_detection', 'mail_triage'
  )),
  CONSTRAINT tenant_decision_usages_mode CHECK (mode IN ('off', 'observe', 'active')),
  CONSTRAINT tenant_decision_usages_threshold CHECK (threshold >= 0 AND threshold <= 1)
);

ALTER TABLE tenant_decision_usages ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_decision_usages AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_decision_usages TO mimorya_app;

-- The questions asked of the decision service, among the roles of the
-- model calls.
ALTER TABLE usage_records DROP CONSTRAINT usage_records_role_check;
ALTER TABLE usage_records
  ADD CONSTRAINT usage_records_role_check
    CHECK (role IN ('main', 'escalation', 'classification', 'advisor', 'review', 'decision'));
