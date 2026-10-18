-- The audit log references tenants, users and runs. Deleting one of them would
-- delete (tenant) or rewrite (user, run) audit entries through the foreign
-- keys' referential actions, which run whatever the privileges on audit_log.
-- The application never deletes these rows: only the owner role does (demo
-- seed reset, future erasure procedures that keep the trace).
REVOKE DELETE ON tenants, users, agent_runs FROM mimorya_app;
