-- The model provider a company chose for its own runs (ADR-024). Without a
-- row, runs use the platform's provider. The API key is never stored here:
-- it is a secret (`secrets`, name `llm.api_key`), encrypted by the API.
CREATE TABLE tenant_llm_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  provider text NOT NULL,
  model text NOT NULL,
  base_url text,
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_llm_settings_provider_check CHECK (
    provider IN ('anthropic', 'openai', 'openrouter', 'gemini', 'ollama', 'openai_compatible')
  )
);

ALTER TABLE tenant_llm_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_llm_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_llm_settings TO mimorya_app;
