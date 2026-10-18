/** Missions: work entrusted to the agent and followed over time. */
import { AUTONOMY_LEVELS } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import { check, index, jsonb, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps } from './common';
import { tenants, users } from './tenancy';

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
