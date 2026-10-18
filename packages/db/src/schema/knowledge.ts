/** Multi-level memory and versioned skills. */
import { MEMORY_KINDS, MEMORY_SCOPES, SENSITIVITY_LEVELS } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import {
  type AnyPgColumn,
  check,
  index,
  integer,
  jsonb,
  pgTable,
  real,
  text,
  timestamp,
  uniqueIndex,
  uuid,
  vector,
} from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps, tsvector } from './common';
import { orgUnits, tenants, users } from './tenancy';
import { agentRuns } from './runs';

export const EMBEDDING_DIMENSIONS = 1024;

export interface MemorySource {
  type: 'conversation' | 'run' | 'document' | 'manual';
  id?: string;
  label?: string;
}

export const memoryItems = pgTable(
  'memory_items',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    scope: text('scope').notNull(),
    ownerUserId: uuid('owner_user_id').references(() => users.id, { onDelete: 'cascade' }),
    orgUnitId: uuid('org_unit_id').references(() => orgUnits.id, { onDelete: 'cascade' }),
    kind: text('kind').notNull(),
    title: text('title').notNull(),
    content: text('content').notNull(),
    sensitivity: text('sensitivity').notNull().default('internal'),
    status: text('status').notNull().default('active'),
    source: jsonb('source').$type<MemorySource>().notNull(),
    confidence: real('confidence').notNull().default(1),
    createdByUserId: uuid('created_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    createdByRunId: uuid('created_by_run_id').references(() => agentRuns.id, {
      onDelete: 'set null',
    }),
    expiresAt: timestamp('expires_at', { withTimezone: true }),
    lastUsedAt: timestamp('last_used_at', { withTimezone: true }),
    useCount: integer('use_count').notNull().default(0),
    contentTsv: tsvector('content_tsv').generatedAlwaysAs(
      sql`to_tsvector('french', title || ' ' || content)`,
    ),
    embedding: vector('embedding', { dimensions: EMBEDDING_DIMENSIONS }),
    embeddingModel: text('embedding_model'),
    ...timestamps,
  },
  (t) => [
    index('memory_items_owner_idx').on(t.tenantId, t.scope, t.ownerUserId),
    index('memory_items_unit_idx').on(t.tenantId, t.orgUnitId),
    index('memory_items_tsv_idx').using('gin', t.contentTsv),
    index('memory_items_embedding_idx').using('hnsw', t.embedding.op('vector_cosine_ops')),
    check('memory_items_scope_check', oneOf(sql`${t.scope}`, MEMORY_SCOPES)),
    check('memory_items_kind_check', oneOf(sql`${t.kind}`, MEMORY_KINDS)),
    check('memory_items_sensitivity_check', oneOf(sql`${t.sensitivity}`, SENSITIVITY_LEVELS)),
    check('memory_items_status_check', oneOf(sql`${t.status}`, ['active', 'proposed', 'archived'])),
    check(
      'memory_items_scope_owner_check',
      sql`(${t.scope} <> 'user' or ${t.ownerUserId} is not null) and (${t.scope} <> 'org_unit' or ${t.orgUnitId} is not null)`,
    ),
    tenantIsolation(),
  ],
);

export const skills = pgTable(
  'skills',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    name: text('name').notNull(),
    description: text('description').notNull(),
    status: text('status').notNull().default('draft'),
    activeVersionId: uuid('active_version_id').references((): AnyPgColumn => skillVersions.id, {
      onDelete: 'set null',
    }),
    orgUnitId: uuid('org_unit_id').references(() => orgUnits.id, { onDelete: 'set null' }),
    createdByUserId: uuid('created_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    ...timestamps,
  },
  (t) => [
    uniqueIndex('skills_tenant_name_key').on(t.tenantId, t.name),
    check('skills_status_check', oneOf(sql`${t.status}`, ['draft', 'active', 'retired'])),
    check('skills_name_check', sql`${t.name} ~ '^[a-z0-9]+(-[a-z0-9]+)*$'`),
    tenantIsolation(),
  ],
);

export const skillVersions = pgTable(
  'skill_versions',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    skillId: uuid('skill_id')
      .notNull()
      .references(() => skills.id, { onDelete: 'cascade' }),
    version: integer('version').notNull(),
    content: text('content').notNull(),
    frontmatter: jsonb('frontmatter').$type<Record<string, unknown>>().notNull().default({}),
    changelog: text('changelog').notNull().default(''),
    status: text('status').notNull().default('draft'),
    createdByUserId: uuid('created_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    createdByRunId: uuid('created_by_run_id').references(() => agentRuns.id, {
      onDelete: 'set null',
    }),
    approvedByUserId: uuid('approved_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    approvedAt: timestamp('approved_at', { withTimezone: true }),
    evalSummary: jsonb('eval_summary').$type<Record<string, unknown>>(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    uniqueIndex('skill_versions_skill_version_key').on(t.skillId, t.version),
    check(
      'skill_versions_status_check',
      oneOf(sql`${t.status}`, ['draft', 'candidate', 'active', 'rejected', 'retired']),
    ),
    tenantIsolation(),
  ],
);

export const skillEvals = pgTable(
  'skill_evals',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    skillId: uuid('skill_id')
      .notNull()
      .references(() => skills.id, { onDelete: 'cascade' }),
    name: text('name').notNull(),
    input: jsonb('input').$type<Record<string, unknown>>().notNull(),
    successCriteria: jsonb('success_criteria').$type<string[]>().notNull(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [index('skill_evals_skill_idx').on(t.skillId), tenantIsolation()],
);
