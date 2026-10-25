-- Searching documents by meaning too (ADR-096): each passage gets the
-- vector of the embedding model its company chose for its memory
-- (ADR-074), computed in the background like a memory's, `embedding_model`
-- set without a vector when the model refused its text. A passage is never
-- otherwise changed: only these two columns may be updated. Its vector goes
-- with it, and every vector of the company is wiped when it goes back to
-- searching by words. The vectors are compared exactly, over the passages
-- the reader may read only: no approximate index.
ALTER TABLE document_chunks
  ADD COLUMN embedding vector(1024),
  ADD COLUMN embedding_model text;

GRANT UPDATE (embedding, embedding_model) ON document_chunks TO mimorya_app;

-- The active companies whose active memories, or document passages, await
-- a vector of the model they chose. Only the company crosses the
-- companies' boundary.
CREATE OR REPLACE FUNCTION system_tenants_awaiting_embeddings()
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
     AND (EXISTS (
           SELECT 1 FROM memory_items
            WHERE memory_items.tenant_id = settings.tenant_id
              AND memory_items.status = 'active'
              AND memory_items.embedding_model IS DISTINCT FROM settings.model)
       OR EXISTS (
           SELECT 1 FROM document_chunks
            WHERE document_chunks.tenant_id = settings.tenant_id
              AND document_chunks.embedding_model IS DISTINCT FROM settings.model))
$$;

REVOKE ALL ON FUNCTION system_tenants_awaiting_embeddings() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_tenants_awaiting_embeddings() TO mimorya_app;
