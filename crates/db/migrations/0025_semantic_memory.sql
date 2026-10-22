-- The embedding model a company chose to search its memory by meaning
-- (ADR-074). Without a row, memory is searched by its words only: nothing
-- of it leaves for an embedding provider. The API key is never stored here:
-- it is a secret (`secrets`, name `embedding.api_key`), encrypted by the API.
-- A memory is close to a request when the cosine similarity of their
-- vectors reaches `min_similarity`.
CREATE TABLE tenant_embedding_settings (
  tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  provider text NOT NULL,
  model text NOT NULL,
  base_url text,
  min_similarity real NOT NULL,
  updated_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT tenant_embedding_settings_provider_check CHECK (
    provider IN ('mistral', 'openai', 'ollama', 'openai_compatible')
  ),
  CONSTRAINT tenant_embedding_settings_similarity_check CHECK (
    min_similarity > 0 AND min_similarity < 1
  )
);

ALTER TABLE tenant_embedding_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenant_embedding_settings AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON tenant_embedding_settings TO mimorya_app;

-- The active companies whose active memories await a vector of the model
-- they chose: written or approved since, or embedded by another model
-- before. The API then embeds them in each company. Only the company
-- crosses the companies' boundary.
CREATE FUNCTION system_tenants_awaiting_embeddings()
RETURNS SETOF uuid
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT settings.tenant_id
    FROM tenant_embedding_settings settings
    JOIN tenants ON tenants.id = settings.tenant_id
   WHERE tenants.status = 'active'
     AND EXISTS (
           SELECT 1 FROM memory_items
            WHERE memory_items.tenant_id = settings.tenant_id
              AND memory_items.status = 'active'
              AND memory_items.embedding_model IS DISTINCT FROM settings.model)
$$;

REVOKE ALL ON FUNCTION system_tenants_awaiting_embeddings() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_tenants_awaiting_embeddings() TO mimorya_app;
