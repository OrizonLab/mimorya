-- The record of processing activities (ADR-217): the platform draws it from
-- each company's settings when asked, and stores nothing of it. Reading it
-- is a permission of its own, `compliance:read`, company-wide and never
-- delegated to the agent (crates/core/src/authz/permissions.rs).
--
-- The platform's administrator role of the companies already there gains
-- what the new ones are created with: reading the register.
UPDATE roles SET permissions = permissions || ARRAY['compliance:read']
  WHERE is_system AND key = 'tenant_admin'
    AND NOT 'compliance:read' = ANY (permissions);
