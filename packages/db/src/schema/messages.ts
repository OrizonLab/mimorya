/** Messages of a conversation (full-text indexed for searching past exchanges). */
import { sql } from 'drizzle-orm';
import { check, index, jsonb, pgTable, text, timestamp, uuid } from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, tsvector } from './common';
import { conversations } from './conversations';
import { agentRuns } from './runs';
import { tenants, users } from './tenancy';

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
