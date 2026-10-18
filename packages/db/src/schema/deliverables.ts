/** Deliverables produced by the agent (drafts, plannings, reports). */
import { sql } from 'drizzle-orm';
import { check, index, jsonb, pgTable, text, uuid } from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps } from './common';
import { agentRuns } from './runs';
import { orgUnits, tenants, users } from './tenancy';

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
