/** Conversations, missions, agent runs and everything they produce. */
import { AUTONOMY_LEVELS, TOOL_RISKS } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import {
  bigserial,
  check,
  foreignKey,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps, tsvector } from './common';
import { orgUnits, tenants, users } from './tenancy';

export const RUN_STATUSES = [
  'queued',
  'running',
  'awaiting_approval',
  'completed',
  'failed',
  'cancelled',
] as const;
export type RunStatus = (typeof RUN_STATUSES)[number];

export const APPROVAL_STATUSES = [
  'pending',
  'approved',
  'rejected',
  'expired',
  'cancelled',
] as const;
export type ApprovalStatus = (typeof APPROVAL_STATUSES)[number];

export const conversations = pgTable(
  'conversations',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    ownerUserId: uuid('owner_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    title: text('title').notNull().default('Nouvelle conversation'),
    visibility: text('visibility').notNull().default('private'),
    orgUnitId: uuid('org_unit_id').references(() => orgUnits.id, { onDelete: 'set null' }),
    archivedAt: timestamp('archived_at', { withTimezone: true }),
    ...timestamps,
  },
  (t) => [
    index('conversations_owner_idx').on(t.tenantId, t.ownerUserId, t.updatedAt),
    check('conversations_visibility_check', oneOf(sql`${t.visibility}`, ['private', 'org_unit'])),
    tenantIsolation(),
  ],
);

export const missions = pgTable(
  'missions',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    ownerUserId: uuid('owner_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    title: text('title').notNull(),
    description: text('description').notNull(),
    status: text('status').notNull().default('open'),
    successCriteria: jsonb('success_criteria').$type<string[]>().notNull().default([]),
    autonomyLevel: text('autonomy_level').notNull().default('execute_with_approval'),
    scheduleCron: text('schedule_cron'),
    nextRunAt: timestamp('next_run_at', { withTimezone: true }),
    ...timestamps,
  },
  (t) => [
    index('missions_owner_idx').on(t.tenantId, t.ownerUserId),
    check(
      'missions_status_check',
      oneOf(sql`${t.status}`, [
        'open',
        'in_progress',
        'awaiting_approval',
        'done',
        'failed',
        'cancelled',
      ]),
    ),
    check('missions_autonomy_check', oneOf(sql`${t.autonomyLevel}`, AUTONOMY_LEVELS)),
    tenantIsolation(),
  ],
);

export interface RunBudget {
  maxSteps: number;
  maxInputTokens: number;
  maxOutputTokens: number;
  maxCostMicros: number;
  maxDurationMs: number;
}

export interface RunUsage {
  steps: number;
  toolCalls: number;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export const agentRuns = pgTable(
  'agent_runs',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    conversationId: uuid('conversation_id').references(() => conversations.id, {
      onDelete: 'cascade',
    }),
    missionId: uuid('mission_id').references(() => missions.id, { onDelete: 'set null' }),
    parentRunId: uuid('parent_run_id'),
    initiatedByUserId: uuid('initiated_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    /** The user whose permissions apply to this run. */
    onBehalfOfUserId: uuid('on_behalf_of_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    agentProfile: text('agent_profile').notNull().default('main'),
    status: text('status').notNull().default('queued'),
    objective: text('objective').notNull(),
    successCriteria: jsonb('success_criteria').$type<string[]>().notNull().default([]),
    plan: jsonb('plan').$type<unknown>(),
    autonomyLevel: text('autonomy_level').notNull().default('execute_with_approval'),
    allowedPermissions: text('allowed_permissions').array(),
    budget: jsonb('budget').$type<RunBudget>().notNull(),
    usage: jsonb('usage')
      .$type<RunUsage>()
      .notNull()
      .default({ steps: 0, toolCalls: 0, inputTokens: 0, outputTokens: 0, costMicros: 0 }),
    /** Checkpoint used to resume an interrupted run. */
    state: jsonb('state').$type<unknown>(),
    result: jsonb('result').$type<unknown>(),
    verification: jsonb('verification').$type<unknown>(),
    error: text('error'),
    leaseOwner: text('lease_owner'),
    leaseExpiresAt: timestamp('lease_expires_at', { withTimezone: true }),
    startedAt: timestamp('started_at', { withTimezone: true }),
    finishedAt: timestamp('finished_at', { withTimezone: true }),
    ...timestamps,
  },
  (t) => [
    foreignKey({
      columns: [t.parentRunId],
      foreignColumns: [t.id],
      name: 'agent_runs_parent_fk',
    }).onDelete('cascade'),
    index('agent_runs_tenant_status_idx').on(t.tenantId, t.status),
    index('agent_runs_conversation_idx').on(t.conversationId, t.createdAt),
    index('agent_runs_queue_idx').on(t.status, t.leaseExpiresAt),
    check('agent_runs_status_check', oneOf(sql`${t.status}`, RUN_STATUSES)),
    check('agent_runs_autonomy_check', oneOf(sql`${t.autonomyLevel}`, AUTONOMY_LEVELS)),
    tenantIsolation(),
  ],
);

export const messages = pgTable(
  'messages',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    conversationId: uuid('conversation_id')
      .notNull()
      .references(() => conversations.id, { onDelete: 'cascade' }),
    role: text('role').notNull(),
    authorUserId: uuid('author_user_id').references(() => users.id, { onDelete: 'set null' }),
    content: text('content').notNull(),
    runId: uuid('run_id').references(() => agentRuns.id, { onDelete: 'set null' }),
    metadata: jsonb('metadata').$type<Record<string, unknown>>().notNull().default({}),
    contentTsv: tsvector('content_tsv').generatedAlwaysAs(sql`to_tsvector('french', content)`),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('messages_conversation_idx').on(t.conversationId, t.createdAt),
    index('messages_tsv_idx').using('gin', t.contentTsv),
    check('messages_role_check', oneOf(sql`${t.role}`, ['user', 'assistant'])),
    tenantIsolation(),
  ],
);

export const runEvents = pgTable(
  'run_events',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    runId: uuid('run_id')
      .notNull()
      .references(() => agentRuns.id, { onDelete: 'cascade' }),
    seq: integer('seq').notNull(),
    type: text('type').notNull(),
    data: jsonb('data').$type<Record<string, unknown>>().notNull(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [uniqueIndex('run_events_run_seq_key').on(t.runId, t.seq), tenantIsolation()],
);

export const approvals = pgTable(
  'approvals',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    runId: uuid('run_id')
      .notNull()
      .references(() => agentRuns.id, { onDelete: 'cascade' }),
    /** The user on whose behalf the agent requested the action. */
    requestedForUserId: uuid('requested_for_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    toolName: text('tool_name').notNull(),
    toolCallId: text('tool_call_id').notNull(),
    actionSummary: text('action_summary').notNull(),
    payload: jsonb('payload').$type<unknown>().notNull(),
    risk: text('risk').notNull(),
    reason: text('reason').notNull(),
    status: text('status').notNull().default('pending'),
    decidedByUserId: uuid('decided_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    decidedAt: timestamp('decided_at', { withTimezone: true }),
    decisionComment: text('decision_comment'),
    expiresAt: timestamp('expires_at', { withTimezone: true }),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('approvals_pending_idx').on(t.tenantId, t.status, t.requestedForUserId),
    index('approvals_run_idx').on(t.runId),
    check('approvals_status_check', oneOf(sql`${t.status}`, APPROVAL_STATUSES)),
    check('approvals_risk_check', oneOf(sql`${t.risk}`, TOOL_RISKS)),
    tenantIsolation(),
  ],
);

export const toolInvocations = pgTable(
  'tool_invocations',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    runId: uuid('run_id')
      .notNull()
      .references(() => agentRuns.id, { onDelete: 'cascade' }),
    toolCallId: text('tool_call_id').notNull(),
    toolName: text('tool_name').notNull(),
    input: jsonb('input').$type<unknown>(),
    decision: text('decision').notNull(),
    decisionReason: text('decision_reason'),
    status: text('status').notNull(),
    outputSummary: text('output_summary'),
    error: text('error'),
    durationMs: integer('duration_ms'),
    approvalId: uuid('approval_id').references(() => approvals.id, { onDelete: 'set null' }),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('tool_invocations_run_idx').on(t.runId),
    check(
      'tool_invocations_decision_check',
      oneOf(sql`${t.decision}`, ['allow', 'require_approval', 'deny']),
    ),
    check(
      'tool_invocations_status_check',
      oneOf(sql`${t.status}`, ['succeeded', 'failed', 'denied', 'awaiting_approval', 'rejected']),
    ),
    tenantIsolation(),
  ],
);

export const deliverables = pgTable(
  'deliverables',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    runId: uuid('run_id').references(() => agentRuns.id, { onDelete: 'set null' }),
    ownerUserId: uuid('owner_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    orgUnitId: uuid('org_unit_id').references(() => orgUnits.id, { onDelete: 'set null' }),
    type: text('type').notNull(),
    title: text('title').notNull(),
    content: jsonb('content').$type<unknown>().notNull(),
    status: text('status').notNull().default('draft'),
    ...timestamps,
  },
  (t) => [
    index('deliverables_owner_idx').on(t.tenantId, t.ownerUserId),
    check(
      'deliverables_status_check',
      oneOf(sql`${t.status}`, ['draft', 'approved', 'applied', 'discarded']),
    ),
    tenantIsolation(),
  ],
);
