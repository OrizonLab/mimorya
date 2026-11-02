-- References between company tables carry the company (ADR-167). Each
-- foreign key from a table with tenant_id to another one (but tenants) now
-- compares (tenant_id, id) instead of id alone: a row can no longer point at
-- a row of another company. Row-level security does not cover foreign key
-- checks, which PostgreSQL makes past it; the key itself now refuses what a
-- faulty write would have let through.
--
-- Each referenced table gets a unique (tenant_id, id) key, gateways also
-- (tenant_id, id, kind) for the keys that name the gateway's kind. Each key
-- keeps its name and its action; a key that sets its reference to null on
-- deletion names that column alone (SET NULL (column)), so that the row
-- keeps its company. The three keys that already compared the company
-- (to identity_providers) are left as they were; rate_limit_values, outside
-- any company, too.

ALTER TABLE agent_runs ADD CONSTRAINT agent_runs_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE approvals ADD CONSTRAINT approvals_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE connector_instances ADD CONSTRAINT connector_instances_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE conversations ADD CONSTRAINT conversations_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE documents ADD CONSTRAINT documents_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE gateway_threads ADD CONSTRAINT gateway_threads_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE gateways ADD CONSTRAINT gateways_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE gateways ADD CONSTRAINT gateways_tenant_id_id_kind_key UNIQUE (tenant_id, id, kind);
ALTER TABLE messages ADD CONSTRAINT messages_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE missions ADD CONSTRAINT missions_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE org_units ADD CONSTRAINT org_units_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE roles ADD CONSTRAINT roles_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE run_events ADD CONSTRAINT run_events_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE secrets ADD CONSTRAINT secrets_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE skill_evaluations ADD CONSTRAINT skill_evaluations_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE skill_versions ADD CONSTRAINT skill_versions_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE skills ADD CONSTRAINT skills_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE users ADD CONSTRAINT users_tenant_id_id_key UNIQUE (tenant_id, id);

-- References to agent_runs.
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_parent_fk,
  ADD CONSTRAINT agent_runs_parent_fk FOREIGN KEY (tenant_id, parent_run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE approvals
  DROP CONSTRAINT approvals_run_id_agent_runs_id_fk,
  ADD CONSTRAINT approvals_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE audit_log
  DROP CONSTRAINT audit_log_run_id_agent_runs_id_fk,
  ADD CONSTRAINT audit_log_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (run_id);
ALTER TABLE deliverables
  DROP CONSTRAINT deliverables_run_id_agent_runs_id_fk,
  ADD CONSTRAINT deliverables_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (run_id);
ALTER TABLE inbound_hooks
  DROP CONSTRAINT inbound_hooks_last_run_id_fkey,
  ADD CONSTRAINT inbound_hooks_last_run_id_fkey FOREIGN KEY (tenant_id, last_run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (last_run_id);
ALTER TABLE memory_items
  DROP CONSTRAINT memory_items_created_by_run_id_agent_runs_id_fk,
  ADD CONSTRAINT memory_items_created_by_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, created_by_run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (created_by_run_id);
ALTER TABLE messages
  DROP CONSTRAINT messages_run_id_agent_runs_id_fk,
  ADD CONSTRAINT messages_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (run_id);
ALTER TABLE run_events
  DROP CONSTRAINT run_events_run_id_agent_runs_id_fk,
  ADD CONSTRAINT run_events_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE scheduled_tasks
  DROP CONSTRAINT scheduled_tasks_last_run_id_fkey,
  ADD CONSTRAINT scheduled_tasks_last_run_id_fkey FOREIGN KEY (tenant_id, last_run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (last_run_id);
ALTER TABLE skill_versions
  DROP CONSTRAINT skill_versions_created_by_run_id_agent_runs_id_fk,
  ADD CONSTRAINT skill_versions_created_by_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, created_by_run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (created_by_run_id);
ALTER TABLE tool_invocations
  DROP CONSTRAINT tool_invocations_run_id_agent_runs_id_fk,
  ADD CONSTRAINT tool_invocations_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE usage_records
  DROP CONSTRAINT usage_records_run_id_agent_runs_id_fk,
  ADD CONSTRAINT usage_records_run_id_agent_runs_id_fk FOREIGN KEY (tenant_id, run_id)
    REFERENCES agent_runs (tenant_id, id) ON DELETE SET NULL (run_id);
-- References to approvals.
ALTER TABLE gateway_approval_notices
  DROP CONSTRAINT gateway_approval_notices_approval_id_fkey,
  ADD CONSTRAINT gateway_approval_notices_approval_id_fkey FOREIGN KEY (tenant_id, approval_id)
    REFERENCES approvals (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE tool_invocations
  DROP CONSTRAINT tool_invocations_approval_id_approvals_id_fk,
  ADD CONSTRAINT tool_invocations_approval_id_approvals_id_fk FOREIGN KEY (tenant_id, approval_id)
    REFERENCES approvals (tenant_id, id) ON DELETE SET NULL (approval_id);
-- References to connector_instances.
ALTER TABLE account_links
  DROP CONSTRAINT account_links_connector_id_fkey,
  ADD CONSTRAINT account_links_connector_id_fkey FOREIGN KEY (tenant_id, connector_id)
    REFERENCES connector_instances (tenant_id, id) ON DELETE CASCADE;
-- References to conversations.
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_conversation_id_conversations_id_fk,
  ADD CONSTRAINT agent_runs_conversation_id_conversations_id_fk FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE conversation_summaries
  DROP CONSTRAINT conversation_summaries_conversation_id_fkey,
  ADD CONSTRAINT conversation_summaries_conversation_id_fkey FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_threads
  DROP CONSTRAINT gateway_threads_conversation_id_fkey,
  ADD CONSTRAINT gateway_threads_conversation_id_fkey FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE inbound_hooks
  DROP CONSTRAINT inbound_hooks_last_conversation_id_fkey,
  ADD CONSTRAINT inbound_hooks_last_conversation_id_fkey FOREIGN KEY (tenant_id, last_conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE SET NULL (last_conversation_id);
ALTER TABLE messages
  DROP CONSTRAINT messages_conversation_id_conversations_id_fk,
  ADD CONSTRAINT messages_conversation_id_conversations_id_fk FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE missions
  DROP CONSTRAINT missions_conversation_id_fkey,
  ADD CONSTRAINT missions_conversation_id_fkey FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE scheduled_tasks
  DROP CONSTRAINT scheduled_tasks_last_conversation_id_fkey,
  ADD CONSTRAINT scheduled_tasks_last_conversation_id_fkey FOREIGN KEY (tenant_id, last_conversation_id)
    REFERENCES conversations (tenant_id, id) ON DELETE SET NULL (last_conversation_id);
-- References to documents.
ALTER TABLE document_chunks
  DROP CONSTRAINT document_chunks_document_id_fkey,
  ADD CONSTRAINT document_chunks_document_id_fkey FOREIGN KEY (tenant_id, document_id)
    REFERENCES documents (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE document_files
  DROP CONSTRAINT document_files_document_id_fkey,
  ADD CONSTRAINT document_files_document_id_fkey FOREIGN KEY (tenant_id, document_id)
    REFERENCES documents (tenant_id, id) ON DELETE CASCADE;
-- References to gateway_threads.
ALTER TABLE gateway_cards
  DROP CONSTRAINT gateway_cards_thread_id_fkey,
  ADD CONSTRAINT gateway_cards_thread_id_fkey FOREIGN KEY (tenant_id, thread_id)
    REFERENCES gateway_threads (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_replies
  DROP CONSTRAINT gateway_replies_thread_id_fkey,
  ADD CONSTRAINT gateway_replies_thread_id_fkey FOREIGN KEY (tenant_id, thread_id)
    REFERENCES gateway_threads (tenant_id, id) ON DELETE CASCADE;
-- References to gateways.
ALTER TABLE gateway_approval_notices
  DROP CONSTRAINT gateway_approval_notices_gateway_id_fkey,
  ADD CONSTRAINT gateway_approval_notices_gateway_id_fkey FOREIGN KEY (tenant_id, gateway_id)
    REFERENCES gateways (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_cards
  DROP CONSTRAINT gateway_cards_gateway_id_fkey,
  ADD CONSTRAINT gateway_cards_gateway_id_fkey FOREIGN KEY (tenant_id, gateway_id)
    REFERENCES gateways (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_events
  DROP CONSTRAINT gateway_events_gateway_id_fkey,
  ADD CONSTRAINT gateway_events_gateway_id_fkey FOREIGN KEY (tenant_id, gateway_id)
    REFERENCES gateways (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_identities
  DROP CONSTRAINT gateway_identities_gateway_id_fkey,
  ADD CONSTRAINT gateway_identities_gateway_id_fkey FOREIGN KEY (tenant_id, gateway_id)
    REFERENCES gateways (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_identities
  DROP CONSTRAINT gateway_identities_gateway_kind,
  ADD CONSTRAINT gateway_identities_gateway_kind FOREIGN KEY (tenant_id, gateway_id, kind)
    REFERENCES gateways (tenant_id, id, kind) ON DELETE CASCADE;
ALTER TABLE gateway_link_codes
  DROP CONSTRAINT gateway_link_codes_gateway,
  ADD CONSTRAINT gateway_link_codes_gateway FOREIGN KEY (tenant_id, gateway_id, kind)
    REFERENCES gateways (tenant_id, id, kind) ON DELETE CASCADE;
ALTER TABLE gateway_threads
  DROP CONSTRAINT gateway_threads_gateway_id_fkey,
  ADD CONSTRAINT gateway_threads_gateway_id_fkey FOREIGN KEY (tenant_id, gateway_id)
    REFERENCES gateways (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_threads
  DROP CONSTRAINT gateway_threads_gateway_kind,
  ADD CONSTRAINT gateway_threads_gateway_kind FOREIGN KEY (tenant_id, gateway_id, kind)
    REFERENCES gateways (tenant_id, id, kind) ON DELETE CASCADE;
-- References to messages.
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_request_message_id_fkey,
  ADD CONSTRAINT agent_runs_request_message_id_fkey FOREIGN KEY (tenant_id, request_message_id)
    REFERENCES messages (tenant_id, id) ON DELETE SET NULL (request_message_id);
ALTER TABLE conversation_summaries
  DROP CONSTRAINT conversation_summaries_through_message_id_fkey,
  ADD CONSTRAINT conversation_summaries_through_message_id_fkey FOREIGN KEY (tenant_id, through_message_id)
    REFERENCES messages (tenant_id, id) ON DELETE CASCADE;
-- References to missions.
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_mission_id_missions_id_fk,
  ADD CONSTRAINT agent_runs_mission_id_missions_id_fk FOREIGN KEY (tenant_id, mission_id)
    REFERENCES missions (tenant_id, id) ON DELETE SET NULL (mission_id);
-- References to org_units.
ALTER TABLE conversations
  DROP CONSTRAINT conversations_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT conversations_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE SET NULL (org_unit_id);
ALTER TABLE deliverables
  DROP CONSTRAINT deliverables_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT deliverables_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE SET NULL (org_unit_id);
ALTER TABLE documents
  DROP CONSTRAINT documents_org_unit_id_fkey,
  ADD CONSTRAINT documents_org_unit_id_fkey FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id);
ALTER TABLE knowledge_shares
  DROP CONSTRAINT knowledge_shares_source_org_unit_id_fkey,
  ADD CONSTRAINT knowledge_shares_source_org_unit_id_fkey FOREIGN KEY (tenant_id, source_org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE knowledge_shares
  DROP CONSTRAINT knowledge_shares_target_org_unit_id_fkey,
  ADD CONSTRAINT knowledge_shares_target_org_unit_id_fkey FOREIGN KEY (tenant_id, target_org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE memory_items
  DROP CONSTRAINT memory_items_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT memory_items_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE org_memberships
  DROP CONSTRAINT org_memberships_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT org_memberships_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE org_units
  DROP CONSTRAINT org_units_parent_fk,
  ADD CONSTRAINT org_units_parent_fk FOREIGN KEY (tenant_id, parent_id)
    REFERENCES org_units (tenant_id, id) ON DELETE RESTRICT;
ALTER TABLE role_assignments
  DROP CONSTRAINT role_assignments_scope_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT role_assignments_scope_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, scope_org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE skills
  DROP CONSTRAINT skills_org_unit_id_org_units_id_fk,
  ADD CONSTRAINT skills_org_unit_id_org_units_id_fk FOREIGN KEY (tenant_id, org_unit_id)
    REFERENCES org_units (tenant_id, id) ON DELETE SET NULL (org_unit_id);
-- References to roles.
ALTER TABLE role_assignments
  DROP CONSTRAINT role_assignments_role_id_roles_id_fk,
  ADD CONSTRAINT role_assignments_role_id_roles_id_fk FOREIGN KEY (tenant_id, role_id)
    REFERENCES roles (tenant_id, id) ON DELETE CASCADE;
-- References to run_events.
ALTER TABLE gateway_replies
  DROP CONSTRAINT gateway_replies_event_id_fkey,
  ADD CONSTRAINT gateway_replies_event_id_fkey FOREIGN KEY (tenant_id, event_id)
    REFERENCES run_events (tenant_id, id) ON DELETE CASCADE;
-- References to secrets.
ALTER TABLE connector_instances
  DROP CONSTRAINT connector_instances_secret_id_secrets_id_fk,
  ADD CONSTRAINT connector_instances_secret_id_secrets_id_fk FOREIGN KEY (tenant_id, secret_id)
    REFERENCES secrets (tenant_id, id) ON DELETE SET NULL (secret_id);
-- References to skill_evaluations.
ALTER TABLE skill_evaluation_results
  DROP CONSTRAINT skill_evaluation_results_evaluation_id_fkey,
  ADD CONSTRAINT skill_evaluation_results_evaluation_id_fkey FOREIGN KEY (tenant_id, evaluation_id)
    REFERENCES skill_evaluations (tenant_id, id) ON DELETE CASCADE;
-- References to skill_versions.
ALTER TABLE skills
  DROP CONSTRAINT skills_active_version_id_skill_versions_id_fk,
  ADD CONSTRAINT skills_active_version_id_skill_versions_id_fk FOREIGN KEY (tenant_id, active_version_id)
    REFERENCES skill_versions (tenant_id, id) ON DELETE SET NULL (active_version_id);
-- References to skills.
ALTER TABLE skill_eval_cases
  DROP CONSTRAINT skill_eval_cases_skill_id_fkey,
  ADD CONSTRAINT skill_eval_cases_skill_id_fkey FOREIGN KEY (tenant_id, skill_id)
    REFERENCES skills (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE skill_evaluations
  DROP CONSTRAINT skill_evaluations_skill_id_fkey,
  ADD CONSTRAINT skill_evaluations_skill_id_fkey FOREIGN KEY (tenant_id, skill_id)
    REFERENCES skills (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE skill_versions
  DROP CONSTRAINT skill_versions_skill_id_skills_id_fk,
  ADD CONSTRAINT skill_versions_skill_id_skills_id_fk FOREIGN KEY (tenant_id, skill_id)
    REFERENCES skills (tenant_id, id) ON DELETE CASCADE;
-- References to users.
ALTER TABLE account_links
  DROP CONSTRAINT account_links_user_id_fkey,
  ADD CONSTRAINT account_links_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_initiated_by_user_id_users_id_fk,
  ADD CONSTRAINT agent_runs_initiated_by_user_id_users_id_fk FOREIGN KEY (tenant_id, initiated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (initiated_by_user_id);
ALTER TABLE agent_runs
  DROP CONSTRAINT agent_runs_on_behalf_of_user_id_users_id_fk,
  ADD CONSTRAINT agent_runs_on_behalf_of_user_id_users_id_fk FOREIGN KEY (tenant_id, on_behalf_of_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE approval_delegations
  DROP CONSTRAINT approval_delegations_declared_by_user_id_fkey,
  ADD CONSTRAINT approval_delegations_declared_by_user_id_fkey FOREIGN KEY (tenant_id, declared_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (declared_by_user_id);
ALTER TABLE approval_delegations
  DROP CONSTRAINT approval_delegations_delegate_user_id_fkey,
  ADD CONSTRAINT approval_delegations_delegate_user_id_fkey FOREIGN KEY (tenant_id, delegate_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE approval_delegations
  DROP CONSTRAINT approval_delegations_user_id_fkey,
  ADD CONSTRAINT approval_delegations_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE approval_settings
  DROP CONSTRAINT approval_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT approval_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE approvals
  DROP CONSTRAINT approvals_approver_user_id_fkey,
  ADD CONSTRAINT approvals_approver_user_id_fkey FOREIGN KEY (tenant_id, approver_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (approver_user_id);
ALTER TABLE approvals
  DROP CONSTRAINT approvals_decided_by_user_id_users_id_fk,
  ADD CONSTRAINT approvals_decided_by_user_id_users_id_fk FOREIGN KEY (tenant_id, decided_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (decided_by_user_id);
ALTER TABLE approvals
  DROP CONSTRAINT approvals_requested_for_user_id_users_id_fk,
  ADD CONSTRAINT approvals_requested_for_user_id_users_id_fk FOREIGN KEY (tenant_id, requested_for_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE audit_log
  DROP CONSTRAINT audit_log_actor_user_id_users_id_fk,
  ADD CONSTRAINT audit_log_actor_user_id_users_id_fk FOREIGN KEY (tenant_id, actor_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (actor_user_id);
ALTER TABLE audit_log
  DROP CONSTRAINT audit_log_on_behalf_of_user_id_users_id_fk,
  ADD CONSTRAINT audit_log_on_behalf_of_user_id_users_id_fk FOREIGN KEY (tenant_id, on_behalf_of_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (on_behalf_of_user_id);
ALTER TABLE conversations
  DROP CONSTRAINT conversations_owner_user_id_users_id_fk,
  ADD CONSTRAINT conversations_owner_user_id_users_id_fk FOREIGN KEY (tenant_id, owner_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE deliverables
  DROP CONSTRAINT deliverables_owner_user_id_users_id_fk,
  ADD CONSTRAINT deliverables_owner_user_id_users_id_fk FOREIGN KEY (tenant_id, owner_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE documents
  DROP CONSTRAINT documents_replaced_by_user_id_fkey,
  ADD CONSTRAINT documents_replaced_by_user_id_fkey FOREIGN KEY (tenant_id, replaced_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (replaced_by_user_id);
ALTER TABLE documents
  DROP CONSTRAINT documents_uploaded_by_user_id_fkey,
  ADD CONSTRAINT documents_uploaded_by_user_id_fkey FOREIGN KEY (tenant_id, uploaded_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (uploaded_by_user_id);
ALTER TABLE gateway_approval_notices
  DROP CONSTRAINT gateway_approval_notices_user_id_fkey,
  ADD CONSTRAINT gateway_approval_notices_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_cards
  DROP CONSTRAINT gateway_cards_user_id_fkey,
  ADD CONSTRAINT gateway_cards_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_identities
  DROP CONSTRAINT gateway_identities_user_id_fkey,
  ADD CONSTRAINT gateway_identities_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_link_codes
  DROP CONSTRAINT gateway_link_codes_user_id_fkey,
  ADD CONSTRAINT gateway_link_codes_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE gateway_threads
  DROP CONSTRAINT gateway_threads_user_id_fkey,
  ADD CONSTRAINT gateway_threads_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE identity_providers
  DROP CONSTRAINT identity_providers_created_by_user_id_fkey,
  ADD CONSTRAINT identity_providers_created_by_user_id_fkey FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE inbound_hooks
  DROP CONSTRAINT inbound_hooks_user_id_fkey,
  ADD CONSTRAINT inbound_hooks_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE knowledge_shares
  DROP CONSTRAINT knowledge_shares_created_by_user_id_fkey,
  ADD CONSTRAINT knowledge_shares_created_by_user_id_fkey FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE memory_items
  DROP CONSTRAINT memory_items_created_by_user_id_users_id_fk,
  ADD CONSTRAINT memory_items_created_by_user_id_users_id_fk FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE memory_items
  DROP CONSTRAINT memory_items_owner_user_id_users_id_fk,
  ADD CONSTRAINT memory_items_owner_user_id_users_id_fk FOREIGN KEY (tenant_id, owner_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE messages
  DROP CONSTRAINT messages_author_user_id_users_id_fk,
  ADD CONSTRAINT messages_author_user_id_users_id_fk FOREIGN KEY (tenant_id, author_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (author_user_id);
ALTER TABLE missions
  DROP CONSTRAINT missions_owner_user_id_users_id_fk,
  ADD CONSTRAINT missions_owner_user_id_users_id_fk FOREIGN KEY (tenant_id, owner_user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE org_memberships
  DROP CONSTRAINT org_memberships_user_id_users_id_fk,
  ADD CONSTRAINT org_memberships_user_id_users_id_fk FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE org_units
  DROP CONSTRAINT org_units_lead_user_id_users_id_fk,
  ADD CONSTRAINT org_units_lead_user_id_users_id_fk FOREIGN KEY (tenant_id, lead_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (lead_user_id);
ALTER TABLE password_resets
  DROP CONSTRAINT password_resets_user_id_fkey,
  ADD CONSTRAINT password_resets_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE role_assignments
  DROP CONSTRAINT role_assignments_granted_by_user_id_users_id_fk,
  ADD CONSTRAINT role_assignments_granted_by_user_id_users_id_fk FOREIGN KEY (tenant_id, granted_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (granted_by_user_id);
ALTER TABLE role_assignments
  DROP CONSTRAINT role_assignments_user_id_users_id_fk,
  ADD CONSTRAINT role_assignments_user_id_users_id_fk FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE scheduled_tasks
  DROP CONSTRAINT scheduled_tasks_user_id_fkey,
  ADD CONSTRAINT scheduled_tasks_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE sessions
  DROP CONSTRAINT sessions_user_id_users_id_fk,
  ADD CONSTRAINT sessions_user_id_users_id_fk FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE skill_eval_cases
  DROP CONSTRAINT skill_eval_cases_created_by_user_id_fkey,
  ADD CONSTRAINT skill_eval_cases_created_by_user_id_fkey FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE skill_evaluations
  DROP CONSTRAINT skill_evaluations_launched_by_user_id_fkey,
  ADD CONSTRAINT skill_evaluations_launched_by_user_id_fkey FOREIGN KEY (tenant_id, launched_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (launched_by_user_id);
ALTER TABLE skill_versions
  DROP CONSTRAINT skill_versions_approved_by_user_id_users_id_fk,
  ADD CONSTRAINT skill_versions_approved_by_user_id_users_id_fk FOREIGN KEY (tenant_id, approved_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (approved_by_user_id);
ALTER TABLE skill_versions
  DROP CONSTRAINT skill_versions_created_by_user_id_users_id_fk,
  ADD CONSTRAINT skill_versions_created_by_user_id_users_id_fk FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE skills
  DROP CONSTRAINT skills_created_by_user_id_users_id_fk,
  ADD CONSTRAINT skills_created_by_user_id_users_id_fk FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE tenant_embedding_settings
  DROP CONSTRAINT tenant_embedding_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT tenant_embedding_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE tenant_evaluation_settings
  DROP CONSTRAINT tenant_evaluation_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT tenant_evaluation_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE tenant_llm_settings
  DROP CONSTRAINT tenant_llm_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT tenant_llm_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE tenant_memory_settings
  DROP CONSTRAINT tenant_memory_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT tenant_memory_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE tenant_mission_settings
  DROP CONSTRAINT tenant_mission_settings_updated_by_user_id_fkey,
  ADD CONSTRAINT tenant_mission_settings_updated_by_user_id_fkey FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (updated_by_user_id);
ALTER TABLE usage_records
  DROP CONSTRAINT usage_records_user_id_users_id_fk,
  ADD CONSTRAINT usage_records_user_id_users_id_fk FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (user_id);
ALTER TABLE user_identities
  DROP CONSTRAINT user_identities_user_id_fkey,
  ADD CONSTRAINT user_identities_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE user_invitations
  DROP CONSTRAINT user_invitations_created_by_user_id_fkey,
  ADD CONSTRAINT user_invitations_created_by_user_id_fkey FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (created_by_user_id);
ALTER TABLE user_invitations
  DROP CONSTRAINT user_invitations_user_id_fkey,
  ADD CONSTRAINT user_invitations_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE user_notification_settings
  DROP CONSTRAINT user_notification_settings_user_id_fkey,
  ADD CONSTRAINT user_notification_settings_user_id_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES users (tenant_id, id) ON DELETE CASCADE;
ALTER TABLE verified_domains
  DROP CONSTRAINT verified_domains_verified_by_user_id_fkey,
  ADD CONSTRAINT verified_domains_verified_by_user_id_fkey FOREIGN KEY (tenant_id, verified_by_user_id)
    REFERENCES users (tenant_id, id) ON DELETE SET NULL (verified_by_user_id);
