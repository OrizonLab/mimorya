CREATE TABLE "org_memberships" (
	"tenant_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"org_unit_id" uuid NOT NULL,
	"is_primary" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "org_memberships_user_id_org_unit_id_pk" PRIMARY KEY("user_id","org_unit_id")
);

ALTER TABLE "org_memberships" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "org_units" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"parent_id" uuid,
	"name" text NOT NULL,
	"type" text NOT NULL,
	"lead_user_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "org_units_type_check" CHECK ("org_units"."type" in ('company', 'site', 'direction', 'department', 'service', 'team'))
);

ALTER TABLE "org_units" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "sessions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"token_hash" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_seen_at" timestamp with time zone DEFAULT now() NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"absolute_expires_at" timestamp with time zone NOT NULL,
	"revoked_at" timestamp with time zone,
	"ip" "inet",
	"user_agent" text
);

ALTER TABLE "sessions" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "tenants" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"slug" text NOT NULL,
	"name" text NOT NULL,
	"status" text DEFAULT 'active' NOT NULL,
	"settings" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "tenants_status_check" CHECK ("tenants"."status" in ('active', 'suspended'))
);

ALTER TABLE "tenants" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "users" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"email" "citext" NOT NULL,
	"display_name" text NOT NULL,
	"job_title" text,
	"status" text DEFAULT 'active' NOT NULL,
	"password_hash" text,
	"clearance" text DEFAULT 'internal' NOT NULL,
	"attributes" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"locale" text DEFAULT 'fr' NOT NULL,
	"last_login_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "users_status_check" CHECK ("users"."status" in ('active', 'invited', 'disabled')),
	CONSTRAINT "users_clearance_check" CHECK ("users"."clearance" in ('public', 'internal', 'confidential', 'restricted'))
);

ALTER TABLE "users" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "access_policies" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"description" text NOT NULL,
	"effect" text NOT NULL,
	"permissions" text[] NOT NULL,
	"condition" jsonb NOT NULL,
	"enabled" boolean DEFAULT true NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "access_policies_effect_check" CHECK ("access_policies"."effect" in ('allow', 'deny'))
);

ALTER TABLE "access_policies" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "role_assignments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"role_id" uuid NOT NULL,
	"scope_type" text NOT NULL,
	"scope_org_unit_id" uuid,
	"include_descendants" boolean DEFAULT true NOT NULL,
	"granted_by_user_id" uuid,
	"expires_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "role_assignments_scope_type_check" CHECK ("role_assignments"."scope_type" in ('tenant', 'org_unit', 'self')),
	CONSTRAINT "role_assignments_scope_unit_check" CHECK (("role_assignments"."scope_type" = 'org_unit') = ("role_assignments"."scope_org_unit_id" is not null))
);

ALTER TABLE "role_assignments" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "roles" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"key" text NOT NULL,
	"name" text NOT NULL,
	"description" text DEFAULT '' NOT NULL,
	"permissions" text[] DEFAULT '{}'::text[] NOT NULL,
	"is_system" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE "roles" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "agent_runs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"conversation_id" uuid,
	"mission_id" uuid,
	"parent_run_id" uuid,
	"initiated_by_user_id" uuid,
	"on_behalf_of_user_id" uuid NOT NULL,
	"agent_profile" text DEFAULT 'main' NOT NULL,
	"status" text DEFAULT 'queued' NOT NULL,
	"objective" text NOT NULL,
	"success_criteria" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"plan" jsonb,
	"autonomy_level" text DEFAULT 'execute_with_approval' NOT NULL,
	"allowed_permissions" text[],
	"budget" jsonb NOT NULL,
	"usage" jsonb DEFAULT '{"steps":0,"toolCalls":0,"inputTokens":0,"outputTokens":0,"costMicros":0}'::jsonb NOT NULL,
	"state" jsonb,
	"result" jsonb,
	"verification" jsonb,
	"error" text,
	"lease_owner" text,
	"lease_expires_at" timestamp with time zone,
	"started_at" timestamp with time zone,
	"finished_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "agent_runs_status_check" CHECK ("agent_runs"."status" in ('queued', 'running', 'awaiting_approval', 'completed', 'failed', 'cancelled')),
	CONSTRAINT "agent_runs_autonomy_check" CHECK ("agent_runs"."autonomy_level" in ('read_only', 'propose', 'execute_with_approval', 'autonomous_limited'))
);

ALTER TABLE "agent_runs" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "approvals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"run_id" uuid NOT NULL,
	"requested_for_user_id" uuid NOT NULL,
	"tool_name" text NOT NULL,
	"tool_call_id" text NOT NULL,
	"action_summary" text NOT NULL,
	"payload" jsonb NOT NULL,
	"risk" text NOT NULL,
	"reason" text NOT NULL,
	"status" text DEFAULT 'pending' NOT NULL,
	"decided_by_user_id" uuid,
	"decided_at" timestamp with time zone,
	"decision_comment" text,
	"expires_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "approvals_status_check" CHECK ("approvals"."status" in ('pending', 'approved', 'rejected', 'expired', 'cancelled')),
	CONSTRAINT "approvals_risk_check" CHECK ("approvals"."risk" in ('read', 'draft', 'write', 'external', 'irreversible'))
);

ALTER TABLE "approvals" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "conversations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"owner_user_id" uuid NOT NULL,
	"title" text DEFAULT 'Nouvelle conversation' NOT NULL,
	"visibility" text DEFAULT 'private' NOT NULL,
	"org_unit_id" uuid,
	"archived_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "conversations_visibility_check" CHECK ("conversations"."visibility" in ('private', 'org_unit'))
);

ALTER TABLE "conversations" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "deliverables" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"run_id" uuid,
	"owner_user_id" uuid NOT NULL,
	"org_unit_id" uuid,
	"type" text NOT NULL,
	"title" text NOT NULL,
	"content" jsonb NOT NULL,
	"status" text DEFAULT 'draft' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "deliverables_status_check" CHECK ("deliverables"."status" in ('draft', 'approved', 'applied', 'discarded'))
);

ALTER TABLE "deliverables" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "messages" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"conversation_id" uuid NOT NULL,
	"role" text NOT NULL,
	"author_user_id" uuid,
	"content" text NOT NULL,
	"run_id" uuid,
	"metadata" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"content_tsv" "tsvector" GENERATED ALWAYS AS (to_tsvector('french', content)) STORED,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "messages_role_check" CHECK ("messages"."role" in ('user', 'assistant'))
);

ALTER TABLE "messages" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "missions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"owner_user_id" uuid NOT NULL,
	"title" text NOT NULL,
	"description" text NOT NULL,
	"status" text DEFAULT 'open' NOT NULL,
	"success_criteria" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"autonomy_level" text DEFAULT 'execute_with_approval' NOT NULL,
	"schedule_cron" text,
	"next_run_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "missions_status_check" CHECK ("missions"."status" in ('open', 'in_progress', 'awaiting_approval', 'done', 'failed', 'cancelled')),
	CONSTRAINT "missions_autonomy_check" CHECK ("missions"."autonomy_level" in ('read_only', 'propose', 'execute_with_approval', 'autonomous_limited'))
);

ALTER TABLE "missions" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "run_events" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"tenant_id" uuid NOT NULL,
	"run_id" uuid NOT NULL,
	"seq" integer NOT NULL,
	"type" text NOT NULL,
	"data" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE "run_events" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "tool_invocations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"run_id" uuid NOT NULL,
	"tool_call_id" text NOT NULL,
	"tool_name" text NOT NULL,
	"input" jsonb,
	"decision" text NOT NULL,
	"decision_reason" text,
	"status" text NOT NULL,
	"output_summary" text,
	"error" text,
	"duration_ms" integer,
	"approval_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "tool_invocations_decision_check" CHECK ("tool_invocations"."decision" in ('allow', 'require_approval', 'deny')),
	CONSTRAINT "tool_invocations_status_check" CHECK ("tool_invocations"."status" in ('succeeded', 'failed', 'denied', 'awaiting_approval', 'rejected'))
);

ALTER TABLE "tool_invocations" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "memory_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"scope" text NOT NULL,
	"owner_user_id" uuid,
	"org_unit_id" uuid,
	"kind" text NOT NULL,
	"title" text NOT NULL,
	"content" text NOT NULL,
	"sensitivity" text DEFAULT 'internal' NOT NULL,
	"status" text DEFAULT 'active' NOT NULL,
	"source" jsonb NOT NULL,
	"confidence" real DEFAULT 1 NOT NULL,
	"created_by_user_id" uuid,
	"created_by_run_id" uuid,
	"expires_at" timestamp with time zone,
	"last_used_at" timestamp with time zone,
	"use_count" integer DEFAULT 0 NOT NULL,
	"content_tsv" "tsvector" GENERATED ALWAYS AS (to_tsvector('french', title || ' ' || content)) STORED,
	"embedding" vector(1024),
	"embedding_model" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "memory_items_scope_check" CHECK ("memory_items"."scope" in ('user', 'org_unit', 'tenant', 'experience')),
	CONSTRAINT "memory_items_kind_check" CHECK ("memory_items"."kind" in ('preference', 'fact', 'procedure', 'decision', 'lesson', 'episode')),
	CONSTRAINT "memory_items_sensitivity_check" CHECK ("memory_items"."sensitivity" in ('public', 'internal', 'confidential', 'restricted')),
	CONSTRAINT "memory_items_status_check" CHECK ("memory_items"."status" in ('active', 'proposed', 'archived')),
	CONSTRAINT "memory_items_scope_owner_check" CHECK (("memory_items"."scope" <> 'user' or "memory_items"."owner_user_id" is not null) and ("memory_items"."scope" <> 'org_unit' or "memory_items"."org_unit_id" is not null))
);

ALTER TABLE "memory_items" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "skill_evals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"skill_id" uuid NOT NULL,
	"name" text NOT NULL,
	"input" jsonb NOT NULL,
	"success_criteria" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE "skill_evals" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "skill_versions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"skill_id" uuid NOT NULL,
	"version" integer NOT NULL,
	"content" text NOT NULL,
	"frontmatter" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"changelog" text DEFAULT '' NOT NULL,
	"status" text DEFAULT 'draft' NOT NULL,
	"created_by_user_id" uuid,
	"created_by_run_id" uuid,
	"approved_by_user_id" uuid,
	"approved_at" timestamp with time zone,
	"eval_summary" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "skill_versions_status_check" CHECK ("skill_versions"."status" in ('draft', 'candidate', 'active', 'rejected', 'retired'))
);

ALTER TABLE "skill_versions" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "skills" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"name" text NOT NULL,
	"description" text NOT NULL,
	"status" text DEFAULT 'draft' NOT NULL,
	"active_version_id" uuid,
	"org_unit_id" uuid,
	"created_by_user_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "skills_status_check" CHECK ("skills"."status" in ('draft', 'active', 'retired')),
	CONSTRAINT "skills_name_check" CHECK ("skills"."name" ~ '^[a-z0-9]+(-[a-z0-9]+)*$')
);

ALTER TABLE "skills" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "audit_log" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"tenant_id" uuid NOT NULL,
	"occurred_at" timestamp with time zone DEFAULT now() NOT NULL,
	"actor_type" text NOT NULL,
	"actor_user_id" uuid,
	"on_behalf_of_user_id" uuid,
	"run_id" uuid,
	"action" text NOT NULL,
	"resource_type" text,
	"resource_id" text,
	"outcome" text NOT NULL,
	"reason" text,
	"metadata" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"ip" "inet",
	"request_id" text,
	CONSTRAINT "audit_log_actor_check" CHECK ("audit_log"."actor_type" in ('user', 'agent', 'system')),
	CONSTRAINT "audit_log_outcome_check" CHECK ("audit_log"."outcome" in ('success', 'denied', 'failure'))
);

ALTER TABLE "audit_log" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "connector_instances" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"type" text NOT NULL,
	"name" text NOT NULL,
	"enabled" boolean DEFAULT true NOT NULL,
	"config" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"tool_policies" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"secret_id" uuid,
	"status" text DEFAULT 'ok' NOT NULL,
	"last_error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "connector_instances_status_check" CHECK ("connector_instances"."status" in ('ok', 'error', 'disabled'))
);

ALTER TABLE "connector_instances" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "secrets" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"tenant_id" uuid NOT NULL,
	"name" text NOT NULL,
	"ciphertext" "bytea" NOT NULL,
	"iv" "bytea" NOT NULL,
	"auth_tag" "bytea" NOT NULL,
	"key_version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE "secrets" ENABLE ROW LEVEL SECURITY;
CREATE TABLE "usage_records" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"tenant_id" uuid NOT NULL,
	"user_id" uuid,
	"run_id" uuid,
	"kind" text NOT NULL,
	"provider" text NOT NULL,
	"model" text NOT NULL,
	"input_tokens" integer DEFAULT 0 NOT NULL,
	"output_tokens" integer DEFAULT 0 NOT NULL,
	"cost_micros" bigint DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "usage_records_kind_check" CHECK ("usage_records"."kind" in ('llm', 'embedding', 'tool'))
);

ALTER TABLE "usage_records" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "org_memberships" ADD CONSTRAINT "org_memberships_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "org_memberships" ADD CONSTRAINT "org_memberships_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "org_memberships" ADD CONSTRAINT "org_memberships_org_unit_id_org_units_id_fk" FOREIGN KEY ("org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "org_units" ADD CONSTRAINT "org_units_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "org_units" ADD CONSTRAINT "org_units_lead_user_id_users_id_fk" FOREIGN KEY ("lead_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "org_units" ADD CONSTRAINT "org_units_parent_fk" FOREIGN KEY ("parent_id") REFERENCES "public"."org_units"("id") ON DELETE restrict ON UPDATE no action;
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "users" ADD CONSTRAINT "users_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "access_policies" ADD CONSTRAINT "access_policies_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "role_assignments" ADD CONSTRAINT "role_assignments_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "role_assignments" ADD CONSTRAINT "role_assignments_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "role_assignments" ADD CONSTRAINT "role_assignments_role_id_roles_id_fk" FOREIGN KEY ("role_id") REFERENCES "public"."roles"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "role_assignments" ADD CONSTRAINT "role_assignments_scope_org_unit_id_org_units_id_fk" FOREIGN KEY ("scope_org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "role_assignments" ADD CONSTRAINT "role_assignments_granted_by_user_id_users_id_fk" FOREIGN KEY ("granted_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "roles" ADD CONSTRAINT "roles_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_conversation_id_conversations_id_fk" FOREIGN KEY ("conversation_id") REFERENCES "public"."conversations"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_mission_id_missions_id_fk" FOREIGN KEY ("mission_id") REFERENCES "public"."missions"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_initiated_by_user_id_users_id_fk" FOREIGN KEY ("initiated_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_on_behalf_of_user_id_users_id_fk" FOREIGN KEY ("on_behalf_of_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_parent_fk" FOREIGN KEY ("parent_run_id") REFERENCES "public"."agent_runs"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "approvals" ADD CONSTRAINT "approvals_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "approvals" ADD CONSTRAINT "approvals_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "approvals" ADD CONSTRAINT "approvals_requested_for_user_id_users_id_fk" FOREIGN KEY ("requested_for_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "approvals" ADD CONSTRAINT "approvals_decided_by_user_id_users_id_fk" FOREIGN KEY ("decided_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "conversations" ADD CONSTRAINT "conversations_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "conversations" ADD CONSTRAINT "conversations_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "conversations" ADD CONSTRAINT "conversations_org_unit_id_org_units_id_fk" FOREIGN KEY ("org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "deliverables" ADD CONSTRAINT "deliverables_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "deliverables" ADD CONSTRAINT "deliverables_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "deliverables" ADD CONSTRAINT "deliverables_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "deliverables" ADD CONSTRAINT "deliverables_org_unit_id_org_units_id_fk" FOREIGN KEY ("org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "messages" ADD CONSTRAINT "messages_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "messages" ADD CONSTRAINT "messages_conversation_id_conversations_id_fk" FOREIGN KEY ("conversation_id") REFERENCES "public"."conversations"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "messages" ADD CONSTRAINT "messages_author_user_id_users_id_fk" FOREIGN KEY ("author_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "messages" ADD CONSTRAINT "messages_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "missions" ADD CONSTRAINT "missions_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "missions" ADD CONSTRAINT "missions_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "run_events" ADD CONSTRAINT "run_events_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "run_events" ADD CONSTRAINT "run_events_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "tool_invocations" ADD CONSTRAINT "tool_invocations_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "tool_invocations" ADD CONSTRAINT "tool_invocations_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "tool_invocations" ADD CONSTRAINT "tool_invocations_approval_id_approvals_id_fk" FOREIGN KEY ("approval_id") REFERENCES "public"."approvals"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "memory_items" ADD CONSTRAINT "memory_items_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "memory_items" ADD CONSTRAINT "memory_items_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "memory_items" ADD CONSTRAINT "memory_items_org_unit_id_org_units_id_fk" FOREIGN KEY ("org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "memory_items" ADD CONSTRAINT "memory_items_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "memory_items" ADD CONSTRAINT "memory_items_created_by_run_id_agent_runs_id_fk" FOREIGN KEY ("created_by_run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skill_evals" ADD CONSTRAINT "skill_evals_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "skill_evals" ADD CONSTRAINT "skill_evals_skill_id_skills_id_fk" FOREIGN KEY ("skill_id") REFERENCES "public"."skills"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "skill_versions" ADD CONSTRAINT "skill_versions_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "skill_versions" ADD CONSTRAINT "skill_versions_skill_id_skills_id_fk" FOREIGN KEY ("skill_id") REFERENCES "public"."skills"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "skill_versions" ADD CONSTRAINT "skill_versions_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skill_versions" ADD CONSTRAINT "skill_versions_created_by_run_id_agent_runs_id_fk" FOREIGN KEY ("created_by_run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skill_versions" ADD CONSTRAINT "skill_versions_approved_by_user_id_users_id_fk" FOREIGN KEY ("approved_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skills" ADD CONSTRAINT "skills_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "skills" ADD CONSTRAINT "skills_active_version_id_skill_versions_id_fk" FOREIGN KEY ("active_version_id") REFERENCES "public"."skill_versions"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skills" ADD CONSTRAINT "skills_org_unit_id_org_units_id_fk" FOREIGN KEY ("org_unit_id") REFERENCES "public"."org_units"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "skills" ADD CONSTRAINT "skills_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "audit_log" ADD CONSTRAINT "audit_log_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "audit_log" ADD CONSTRAINT "audit_log_actor_user_id_users_id_fk" FOREIGN KEY ("actor_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "audit_log" ADD CONSTRAINT "audit_log_on_behalf_of_user_id_users_id_fk" FOREIGN KEY ("on_behalf_of_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "audit_log" ADD CONSTRAINT "audit_log_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "connector_instances" ADD CONSTRAINT "connector_instances_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "connector_instances" ADD CONSTRAINT "connector_instances_secret_id_secrets_id_fk" FOREIGN KEY ("secret_id") REFERENCES "public"."secrets"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "secrets" ADD CONSTRAINT "secrets_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "usage_records" ADD CONSTRAINT "usage_records_tenant_id_tenants_id_fk" FOREIGN KEY ("tenant_id") REFERENCES "public"."tenants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "usage_records" ADD CONSTRAINT "usage_records_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "usage_records" ADD CONSTRAINT "usage_records_run_id_agent_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."agent_runs"("id") ON DELETE set null ON UPDATE no action;
CREATE UNIQUE INDEX "org_memberships_one_primary" ON "org_memberships" USING btree ("user_id") WHERE "org_memberships"."is_primary";
CREATE INDEX "org_memberships_unit_idx" ON "org_memberships" USING btree ("tenant_id","org_unit_id");
CREATE INDEX "org_units_tenant_parent_idx" ON "org_units" USING btree ("tenant_id","parent_id");
CREATE UNIQUE INDEX "sessions_token_hash_key" ON "sessions" USING btree ("token_hash");
CREATE INDEX "sessions_user_idx" ON "sessions" USING btree ("tenant_id","user_id");
CREATE UNIQUE INDEX "tenants_slug_key" ON "tenants" USING btree ("slug");
CREATE UNIQUE INDEX "users_email_key" ON "users" USING btree ("email");
CREATE INDEX "users_tenant_idx" ON "users" USING btree ("tenant_id");
CREATE INDEX "access_policies_tenant_idx" ON "access_policies" USING btree ("tenant_id");
CREATE INDEX "role_assignments_user_idx" ON "role_assignments" USING btree ("tenant_id","user_id");
CREATE UNIQUE INDEX "roles_tenant_key" ON "roles" USING btree ("tenant_id","key");
CREATE INDEX "agent_runs_tenant_status_idx" ON "agent_runs" USING btree ("tenant_id","status");
CREATE INDEX "agent_runs_conversation_idx" ON "agent_runs" USING btree ("conversation_id","created_at");
CREATE INDEX "agent_runs_queue_idx" ON "agent_runs" USING btree ("status","lease_expires_at");
CREATE INDEX "approvals_pending_idx" ON "approvals" USING btree ("tenant_id","status","requested_for_user_id");
CREATE INDEX "approvals_run_idx" ON "approvals" USING btree ("run_id");
CREATE INDEX "conversations_owner_idx" ON "conversations" USING btree ("tenant_id","owner_user_id","updated_at");
CREATE INDEX "deliverables_owner_idx" ON "deliverables" USING btree ("tenant_id","owner_user_id");
CREATE INDEX "messages_conversation_idx" ON "messages" USING btree ("conversation_id","created_at");
CREATE INDEX "messages_tsv_idx" ON "messages" USING gin ("content_tsv");
CREATE INDEX "missions_owner_idx" ON "missions" USING btree ("tenant_id","owner_user_id");
CREATE UNIQUE INDEX "run_events_run_seq_key" ON "run_events" USING btree ("run_id","seq");
CREATE INDEX "tool_invocations_run_idx" ON "tool_invocations" USING btree ("run_id");
CREATE INDEX "memory_items_owner_idx" ON "memory_items" USING btree ("tenant_id","scope","owner_user_id");
CREATE INDEX "memory_items_unit_idx" ON "memory_items" USING btree ("tenant_id","org_unit_id");
CREATE INDEX "memory_items_tsv_idx" ON "memory_items" USING gin ("content_tsv");
CREATE INDEX "memory_items_embedding_idx" ON "memory_items" USING hnsw ("embedding" vector_cosine_ops);
CREATE INDEX "skill_evals_skill_idx" ON "skill_evals" USING btree ("skill_id");
CREATE UNIQUE INDEX "skill_versions_skill_version_key" ON "skill_versions" USING btree ("skill_id","version");
CREATE UNIQUE INDEX "skills_tenant_name_key" ON "skills" USING btree ("tenant_id","name");
CREATE INDEX "audit_log_tenant_time_idx" ON "audit_log" USING btree ("tenant_id","occurred_at");
CREATE INDEX "audit_log_run_idx" ON "audit_log" USING btree ("run_id");
CREATE UNIQUE INDEX "connector_instances_tenant_name_key" ON "connector_instances" USING btree ("tenant_id","name");
CREATE UNIQUE INDEX "secrets_tenant_name_key" ON "secrets" USING btree ("tenant_id","name");
CREATE INDEX "usage_records_tenant_time_idx" ON "usage_records" USING btree ("tenant_id","created_at");
CREATE POLICY "tenant_isolation" ON "org_memberships" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "org_units" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "sessions" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "tenants" AS PERMISSIVE FOR ALL TO public USING (id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "users" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "access_policies" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "role_assignments" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "roles" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "agent_runs" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "approvals" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "conversations" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "deliverables" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "messages" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "missions" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "run_events" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "tool_invocations" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "memory_items" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "skill_evals" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "skill_versions" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "skills" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "audit_log" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "connector_instances" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "secrets" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);
CREATE POLICY "tenant_isolation" ON "usage_records" AS PERMISSIVE FOR ALL TO public USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);