-- One Google Workspace client per company (ADR-121): its people link their
-- Workspace account there, as at its Microsoft 365 application, and its
-- tools carry fixed names.
CREATE UNIQUE INDEX connector_instances_one_google_workspace
  ON connector_instances (tenant_id) WHERE type = 'google_workspace';
