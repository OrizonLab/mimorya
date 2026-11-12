-- One standard mail per company (ADR-205): its people link their own
-- account at its servers, and its tools carry fixed names.
CREATE UNIQUE INDEX connector_instances_one_mail_server
  ON connector_instances (tenant_id) WHERE type = 'mail_server';
