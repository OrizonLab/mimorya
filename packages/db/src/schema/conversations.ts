/** Conversations: one thread of exchanges between a user and the agent. */
import { sql } from 'drizzle-orm';
import { check, index, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps } from './common';
import { orgUnits, tenants, users } from './tenancy';

/** Title given to a new conversation until its first message names it. */
export const DEFAULT_CONVERSATION_TITLE = 'Nouvelle conversation';

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
    title: text('title').notNull().default(DEFAULT_CONVERSATION_TITLE),
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
