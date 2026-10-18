/** Every tool call of a run, with the policy decision and its outcome. */
import { sql } from 'drizzle-orm';
import { check, index, integer, jsonb, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';
import { approvals } from './approvals';
import { oneOf, tenantIsolation } from './common';
import { agentRuns } from './runs';
import { tenants } from './tenancy';

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
