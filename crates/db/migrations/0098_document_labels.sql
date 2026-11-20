-- A deposited document's type and confidentiality classified by the
-- decision service (ADR-213, D4).
--
-- Its type, one of a fixed list, set by a person who may write it; none
-- until then.
ALTER TABLE documents
  ADD COLUMN document_type text,
  ADD CONSTRAINT documents_document_type CHECK (document_type IN (
    'contract', 'invoice', 'quote', 'procedure', 'minutes', 'report', 'letter',
    'technical_sheet', 'other'
  ));

-- Its type and sensitivity change now too: raised by the decision, or set
-- by a person who may write it at both levels. Its space, depositor and
-- first deposit still never change.
GRANT UPDATE (document_type, sensitivity) ON documents TO mimorya_app;

-- The documents awaiting their classification: one row per document
-- deposited or replaced while the company's usage was not off, naming the
-- version to classify, deleted once classified (or found replaced, deleted
-- or the usage off). No person in it.
CREATE TABLE document_label_requests (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  document_id uuid NOT NULL,
  version integer NOT NULL CHECK (version >= 1),
  requested_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, document_id),
  CONSTRAINT document_label_requests_document_id_fkey FOREIGN KEY (tenant_id, document_id)
    REFERENCES documents (tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE document_label_requests ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON document_label_requests AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON document_label_requests TO mimorya_app;

-- What a classification leaves for a person who may write the document:
-- a type and a looser level proposed, or the level it held before the
-- decision raised it; for the version classified. Deleted once a person
-- sets the labels or dismisses it, or the document is replaced. No person
-- in it: the audit says who did what.
CREATE TABLE document_label_proposals (
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  document_id uuid NOT NULL,
  version integer NOT NULL CHECK (version >= 1),
  document_type text,
  sensitivity text,
  raised_from text,
  engine text NOT NULL,
  proposed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, document_id),
  CONSTRAINT document_label_proposals_document_id_fkey FOREIGN KEY (tenant_id, document_id)
    REFERENCES documents (tenant_id, id) ON DELETE CASCADE,
  CONSTRAINT document_label_proposals_document_type CHECK (document_type IN (
    'contract', 'invoice', 'quote', 'procedure', 'minutes', 'report', 'letter',
    'technical_sheet', 'other'
  )),
  CONSTRAINT document_label_proposals_sensitivity
    CHECK (sensitivity IN ('public', 'internal', 'confidential', 'restricted')),
  CONSTRAINT document_label_proposals_raised_from
    CHECK (raised_from IN ('public', 'internal', 'confidential', 'restricted')),
  -- A level is either raised or proposed lower, never both.
  CONSTRAINT document_label_proposals_one_level CHECK (sensitivity IS NULL OR raised_from IS NULL),
  CONSTRAINT document_label_proposals_something
    CHECK (document_type IS NOT NULL OR sensitivity IS NOT NULL OR raised_from IS NOT NULL),
  CONSTRAINT document_label_proposals_engine CHECK (engine IN ('main_model', 'jev'))
);

ALTER TABLE document_label_proposals ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON document_label_proposals AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON document_label_proposals TO mimorya_app;

-- The active companies with documents awaiting their classification, for
-- the classifier's pass. Only the company crosses the companies'
-- boundary.
CREATE FUNCTION system_tenants_awaiting_document_labels()
RETURNS SETOF uuid
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
  SELECT DISTINCT requests.tenant_id
    FROM document_label_requests requests
    JOIN tenants ON tenants.id = requests.tenant_id
   WHERE tenants.status = 'active'
$$;

REVOKE ALL ON FUNCTION system_tenants_awaiting_document_labels() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION system_tenants_awaiting_document_labels() TO mimorya_app;
