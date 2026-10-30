-- Knowledge shared between units (ADR-143): a company's administrators of
-- its access policies share a unit's knowledge (the memory filed in its
-- space, the documents deposited there, either or both) with another unit,
-- and if they say so the units under it, up to a sensitivity, until a date
-- if they set one. Read only: what the members of the target read of it is
-- decided by the read filters, never written here. A share goes with either
-- unit; who made it is kept while their account is.
CREATE TABLE knowledge_shares (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  source_org_unit_id uuid NOT NULL REFERENCES org_units(id) ON DELETE CASCADE,
  target_org_unit_id uuid NOT NULL REFERENCES org_units(id) ON DELETE CASCADE,
  include_descendants boolean NOT NULL DEFAULT false,
  shares_memory boolean NOT NULL,
  shares_documents boolean NOT NULL,
  max_sensitivity text NOT NULL,
  expires_at timestamptz,
  created_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT knowledge_shares_units CHECK (source_org_unit_id <> target_org_unit_id),
  CONSTRAINT knowledge_shares_something CHECK (shares_memory OR shares_documents),
  CONSTRAINT knowledge_shares_sensitivity
    CHECK (max_sensitivity IN ('public', 'internal', 'confidential', 'restricted')),
  CONSTRAINT knowledge_shares_once UNIQUE (source_org_unit_id, target_org_unit_id)
);

CREATE INDEX knowledge_shares_tenant_idx ON knowledge_shares (tenant_id);

ALTER TABLE knowledge_shares ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON knowledge_shares AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, DELETE ON knowledge_shares TO mimorya_app;
