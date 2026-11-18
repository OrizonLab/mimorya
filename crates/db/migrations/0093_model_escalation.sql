-- Model escalation (ADR-207): a company's stronger model, called when its
-- mechanisms ask for it, within a monthly budget and a number of
-- escalations per run.
--
-- One row per company, chosen by those who manage its settings. The model
-- is one of the provider of its conversations, with the same key and
-- endpoint: the provider and endpoint it was chosen with are kept, and the
-- choice stops serving once those of the conversations change. The budget
-- is in micro-dollars, as every cost: from 1 to 10,000 USD a month.
CREATE TABLE tenant_model_escalation (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  provider text NOT NULL,
  base_url text,
  model text NOT NULL,
  after_failure boolean NOT NULL DEFAULT false,
  router boolean NOT NULL DEFAULT false,
  advisor boolean NOT NULL DEFAULT false,
  monthly_budget_micros bigint NOT NULL,
  max_per_run integer NOT NULL,
  updated_by_user_id uuid,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_model_escalation_budget
    CHECK (monthly_budget_micros BETWEEN 1000000 AND 10000000000),
  CONSTRAINT tenant_model_escalation_per_run CHECK (max_per_run BETWEEN 1 AND 50),
  CONSTRAINT tenant_model_escalation_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id)
);

ALTER TABLE tenant_model_escalation ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_model_escalation AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_model_escalation TO mimorya_app;

-- An erased account empties the rows it last changed (ADR-189).
CREATE INDEX tenant_model_escalation_updated_by_person
  ON tenant_model_escalation (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;

-- The role of each model call: the main model, the escalation model, a
-- classification of the run, the advisor consulted, or the review of an
-- answer (ADR-108). The calls recorded before are the main model's, the
-- reviews included: nothing told them apart.
ALTER TABLE usage_records
  ADD COLUMN role text NOT NULL DEFAULT 'main'
    CONSTRAINT usage_records_role_check
    CHECK (role IN ('main', 'escalation', 'classification', 'advisor', 'review'));
