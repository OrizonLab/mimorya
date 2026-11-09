-- An account erased (ADR-189): an administrator deletes a disabled account,
-- and with it what the platform keeps on the person. Each table naming an
-- account already deletes its rows with it, or empties the column.

-- An audit entry keeps the people it names. The log is kept five years and
-- sealed (ADR-178); through foreign keys, an account's deletion would set
-- the entry's `actor_user_id` or `on_behalf_of_user_id` to null and break
-- its seal. The keys are replaced by the check they made when the entry
-- was written, as for the run it names (ADR-186): the accounts named exist,
-- in the entry's company. The account may go later; the entry, unchanged,
-- names an identifier that no longer leads to anyone.
ALTER TABLE audit_log DROP CONSTRAINT audit_log_actor_user_id_users_id_fk;
ALTER TABLE audit_log DROP CONSTRAINT audit_log_on_behalf_of_user_id_users_id_fk;

CREATE FUNCTION audit_log_people_known()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
  IF NEW.actor_user_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM users u WHERE u.tenant_id = NEW.tenant_id AND u.id = NEW.actor_user_id
  ) THEN
    RAISE EXCEPTION 'audit entry names no account of its company'
      USING ERRCODE = 'foreign_key_violation', CONSTRAINT = 'audit_log_people_known';
  END IF;
  IF NEW.on_behalf_of_user_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM users u WHERE u.tenant_id = NEW.tenant_id AND u.id = NEW.on_behalf_of_user_id
  ) THEN
    RAISE EXCEPTION 'audit entry names no account of its company'
      USING ERRCODE = 'foreign_key_violation', CONSTRAINT = 'audit_log_people_known';
  END IF;
  RETURN NEW;
END
$$;

-- Run as whoever writes the entry: the application, under row-level
-- security, sees its company's accounts alone; the owner sees every
-- account and compares the company.
CREATE TRIGGER audit_log_people_known
  BEFORE INSERT OR UPDATE OF tenant_id, actor_user_id, on_behalf_of_user_id ON audit_log
  FOR EACH ROW EXECUTE FUNCTION audit_log_people_known();

-- The application deletes an account of its company, under row-level
-- security, only when erasing it; the keys that name it then delete or
-- empty the rows of every other table, as their owner.
GRANT DELETE ON users TO mimorya_app;

-- Each key naming an account is looked up once per account erased: without
-- an index starting with its columns, each would read all of the company's
-- rows of its table, messages and consumption included.
CREATE INDEX agent_runs_initiated_by_person
  ON agent_runs (tenant_id, initiated_by_user_id)
  WHERE initiated_by_user_id IS NOT NULL;
CREATE INDEX agent_runs_on_behalf_of_person
  ON agent_runs (tenant_id, on_behalf_of_user_id)
  WHERE on_behalf_of_user_id IS NOT NULL;
CREATE INDEX approval_delegations_declared_by_person
  ON approval_delegations (tenant_id, declared_by_user_id)
  WHERE declared_by_user_id IS NOT NULL;
CREATE INDEX approval_settings_updated_by_person
  ON approval_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX approvals_approver_person
  ON approvals (tenant_id, approver_user_id)
  WHERE approver_user_id IS NOT NULL;
CREATE INDEX approvals_decided_by_person
  ON approvals (tenant_id, decided_by_user_id)
  WHERE decided_by_user_id IS NOT NULL;
CREATE INDEX approvals_requested_for_person
  ON approvals (tenant_id, requested_for_user_id)
  WHERE requested_for_user_id IS NOT NULL;
CREATE INDEX audit_export_tokens_created_by_person
  ON audit_export_tokens (tenant_id, created_by)
  WHERE created_by IS NOT NULL;
CREATE INDEX connector_authorizations_authorized_by_person
  ON connector_authorizations (tenant_id, authorized_by_user_id)
  WHERE authorized_by_user_id IS NOT NULL;
CREATE INDEX documents_replaced_by_person
  ON documents (tenant_id, replaced_by_user_id)
  WHERE replaced_by_user_id IS NOT NULL;
CREATE INDEX documents_uploaded_by_person
  ON documents (tenant_id, uploaded_by_user_id)
  WHERE uploaded_by_user_id IS NOT NULL;
CREATE INDEX gateway_cards_user_person
  ON gateway_cards (tenant_id, user_id)
  WHERE user_id IS NOT NULL;
CREATE INDEX gateway_link_codes_user_person
  ON gateway_link_codes (tenant_id, user_id)
  WHERE user_id IS NOT NULL;
CREATE INDEX identity_providers_created_by_person
  ON identity_providers (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX knowledge_shares_created_by_person
  ON knowledge_shares (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX memory_items_created_by_person
  ON memory_items (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX memory_items_owner_person
  ON memory_items (tenant_id, owner_user_id)
  WHERE owner_user_id IS NOT NULL;
CREATE INDEX messages_author_person
  ON messages (tenant_id, author_user_id)
  WHERE author_user_id IS NOT NULL;
CREATE INDEX org_units_lead_person
  ON org_units (tenant_id, lead_user_id)
  WHERE lead_user_id IS NOT NULL;
CREATE INDEX policy_proposals_decided_by_person
  ON policy_proposals (tenant_id, decided_by_user_id)
  WHERE decided_by_user_id IS NOT NULL;
CREATE INDEX policy_proposals_proposed_by_person
  ON policy_proposals (tenant_id, proposed_by_user_id)
  WHERE proposed_by_user_id IS NOT NULL;
CREATE INDEX role_assignments_granted_by_person
  ON role_assignments (tenant_id, granted_by_user_id)
  WHERE granted_by_user_id IS NOT NULL;
CREATE INDEX scim_tokens_created_by_person
  ON scim_tokens (tenant_id, created_by)
  WHERE created_by IS NOT NULL;
CREATE INDEX skill_eval_cases_created_by_person
  ON skill_eval_cases (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX skill_evaluations_launched_by_person
  ON skill_evaluations (tenant_id, launched_by_user_id)
  WHERE launched_by_user_id IS NOT NULL;
CREATE INDEX skill_versions_approved_by_person
  ON skill_versions (tenant_id, approved_by_user_id)
  WHERE approved_by_user_id IS NOT NULL;
CREATE INDEX skill_versions_created_by_person
  ON skill_versions (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX skills_created_by_person
  ON skills (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX tenant_embedding_settings_updated_by_person
  ON tenant_embedding_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX tenant_evaluation_settings_updated_by_person
  ON tenant_evaluation_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX tenant_llm_settings_updated_by_person
  ON tenant_llm_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX tenant_memory_settings_updated_by_person
  ON tenant_memory_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX tenant_mission_settings_updated_by_person
  ON tenant_mission_settings (tenant_id, updated_by_user_id)
  WHERE updated_by_user_id IS NOT NULL;
CREATE INDEX usage_records_user_person
  ON usage_records (tenant_id, user_id)
  WHERE user_id IS NOT NULL;
CREATE INDEX user_invitations_created_by_person
  ON user_invitations (tenant_id, created_by_user_id)
  WHERE created_by_user_id IS NOT NULL;
CREATE INDEX verified_domains_verified_by_person
  ON verified_domains (tenant_id, verified_by_user_id)
  WHERE verified_by_user_id IS NOT NULL;

-- The platform's administrator role of the companies already there gains
-- what the new ones are created with: erasing a disabled account.
UPDATE roles SET permissions = permissions || ARRAY['user:erase']
  WHERE is_system AND key = 'tenant_admin'
    AND NOT 'user:erase' = ANY (permissions);
