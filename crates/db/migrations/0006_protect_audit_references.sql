-- The audit log references tenants, users and runs (and runs belong to
-- conversations, deleted with them). Deleting one of them would delete
-- (tenant) or rewrite (user, run) audit entries through the foreign keys'
-- referential actions, which run whatever the privileges on audit_log. The
-- application never deletes these rows (conversations are archived): only the
-- owner role does (demo seed reset, future erasure procedures that keep the
-- trace).
REVOKE DELETE ON tenants, users, conversations, agent_runs FROM mimorya_app;
