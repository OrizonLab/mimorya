/** Agent runs (one execution of the agent) and their replayable event journal. */
import { AUTONOMY_LEVELS } from '@mimorya/core';
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
import { oneOf, tenantIsolation, timestamps } from './common';
import { conversations } from './conversations';
import { missions } from './missions';
import { tenants, users } from './tenancy';

export const RUN_STATUSES = [
  'queued',
  'running',
  'awaiting_approval',
  'completed',
  'failed',
  'cancelled',
] as const;
export type RunStatus = (typeof RUN_STATUSES)[number];

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
