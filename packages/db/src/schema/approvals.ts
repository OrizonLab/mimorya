/** Human validations requested by the agent before sensitive actions. */
import { TOOL_RISKS } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import { check, index, jsonb, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation } from './common';
import { agentRuns } from './runs';
import { tenants, users } from './tenancy';

export const APPROVAL_STATUSES = [
  'pending',
  'approved',
  'rejected',
  'expired',
  'cancelled',
] as const;
export type ApprovalStatus = (typeof APPROVAL_STATUSES)[number];

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
