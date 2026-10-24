-- Documents a company deposits for its agent to read (ADR-095): in the
-- company's space or a unit's, read with `document:read`, deposited and
-- deleted with `document:write`, never above one's clearance. The file as
-- deposited is kept apart (`document_files`), stored out of line and not
-- compressed again (a PDF already is); its text, read at deposit by the
-- extraction process, is kept cut into passages (`document_chunks`), never
-- across a page, searched by its words (lot 2 adds their meaning). A
-- document stays until someone who may write its space deletes it, file
-- and passages with it; it goes with its company. A unit holding
-- documents is not deleted. Its depositor leaving, the document stays the
-- company's.
CREATE TABLE documents (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  scope text NOT NULL CHECK (scope IN ('tenant', 'org_unit')),
  org_unit_id uuid REFERENCES org_units(id),
  title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
  file_name text NOT NULL CHECK (char_length(file_name) BETWEEN 1 AND 255),
  kind text NOT NULL CHECK (kind IN ('pdf', 'docx', 'markdown', 'text')),
  sensitivity text NOT NULL DEFAULT 'internal'
    CHECK (sensitivity IN ('public', 'internal', 'confidential', 'restricted')),
  byte_size integer NOT NULL CHECK (byte_size BETWEEN 1 AND 10485760),
  page_count integer CHECK (page_count >= 1),
  uploaded_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT documents_space CHECK ((scope = 'org_unit') = (org_unit_id IS NOT NULL))
);

CREATE INDEX documents_space_idx ON documents (tenant_id, scope, org_unit_id, created_at DESC);
CREATE INDEX documents_org_unit_idx ON documents (org_unit_id);

CREATE TABLE document_files (
  document_id uuid PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  content bytea NOT NULL
);

ALTER TABLE document_files ALTER COLUMN content SET STORAGE EXTERNAL;

CREATE TABLE document_chunks (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  document_id uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  ordinal integer NOT NULL CHECK (ordinal >= 0),
  page integer CHECK (page >= 1),
  heading text CHECK (char_length(heading) BETWEEN 1 AND 200),
  content text NOT NULL CHECK (char_length(content) BETWEEN 1 AND 2000),
  content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('french', content)) STORED,
  UNIQUE (document_id, ordinal)
);

CREATE INDEX document_chunks_tsv_idx ON document_chunks USING gin (content_tsv);

ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE document_files ENABLE ROW LEVEL SECURITY;
ALTER TABLE document_chunks ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON documents AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY tenant_isolation ON document_files AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY tenant_isolation ON document_chunks AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- Never updated: a document is replaced by depositing it again.
GRANT SELECT, INSERT, DELETE ON documents TO mimorya_app;
GRANT SELECT, INSERT, DELETE ON document_files TO mimorya_app;
GRANT SELECT, INSERT, DELETE ON document_chunks TO mimorya_app;

-- The platform's roles of the companies already there gain what the new
-- ones are created with: every member reads the documents, a knowledge
-- manager deposits them too.
UPDATE roles SET permissions = permissions || ARRAY['document:read']
  WHERE is_system AND key IN ('member', 'knowledge_manager')
    AND NOT 'document:read' = ANY (permissions);
UPDATE roles SET permissions = permissions || ARRAY['document:write']
  WHERE is_system AND key = 'knowledge_manager'
    AND NOT 'document:write' = ANY (permissions);
