import { ORG_UNIT_TYPES, SENSITIVITY_LEVELS } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import {
  boolean,
  check,
  foreignKey,
  index,
  inet,
  jsonb,
  pgTable,
  primaryKey,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from 'drizzle-orm/pg-core';
import { citext, oneOf, tenantIsolation, timestamps } from './common';

export const tenants = pgTable(
  'tenants',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    slug: text('slug').notNull(),
    name: text('name').notNull(),
    status: text('status').notNull().default('active'),
    settings: jsonb('settings').$type<Record<string, unknown>>().notNull().default({}),
    ...timestamps,
  },
  (t) => [
    uniqueIndex('tenants_slug_key').on(t.slug),
    check('tenants_status_check', oneOf(sql`${t.status}`, ['active', 'suspended'])),
    tenantIsolation(sql`id`),
  ],
);

export const users = pgTable(
  'users',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    email: citext('email').notNull(),
    displayName: text('display_name').notNull(),
    jobTitle: text('job_title'),
    status: text('status').notNull().default('active'),
    passwordHash: text('password_hash'),
    clearance: text('clearance').notNull().default('internal'),
    attributes: jsonb('attributes').$type<Record<string, unknown>>().notNull().default({}),
    locale: text('locale').notNull().default('fr'),
    lastLoginAt: timestamp('last_login_at', { withTimezone: true }),
    ...timestamps,
  },
  (t) => [
    uniqueIndex('users_email_key').on(t.email),
    index('users_tenant_idx').on(t.tenantId),
    check('users_status_check', oneOf(sql`${t.status}`, ['active', 'invited', 'disabled'])),
    check('users_clearance_check', oneOf(sql`${t.clearance}`, SENSITIVITY_LEVELS)),
    tenantIsolation(),
  ],
);

export const orgUnits = pgTable(
  'org_units',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    parentId: uuid('parent_id'),
    name: text('name').notNull(),
    type: text('type').notNull(),
    leadUserId: uuid('lead_user_id').references(() => users.id, { onDelete: 'set null' }),
    ...timestamps,
  },
  (t) => [
    foreignKey({
      columns: [t.parentId],
      foreignColumns: [t.id],
      name: 'org_units_parent_fk',
    }).onDelete('restrict'),
    index('org_units_tenant_parent_idx').on(t.tenantId, t.parentId),
    check('org_units_type_check', oneOf(sql`${t.type}`, ORG_UNIT_TYPES)),
    tenantIsolation(),
  ],
);

export const orgMemberships = pgTable(
  'org_memberships',
  {
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    orgUnitId: uuid('org_unit_id')
      .notNull()
      .references(() => orgUnits.id, { onDelete: 'cascade' }),
    isPrimary: boolean('is_primary').notNull().default(false),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    primaryKey({ columns: [t.userId, t.orgUnitId] }),
    uniqueIndex('org_memberships_one_primary')
      .on(t.userId)
      .where(sql`${t.isPrimary}`),
    index('org_memberships_unit_idx').on(t.tenantId, t.orgUnitId),
    tenantIsolation(),
  ],
);

export const sessions = pgTable(
  'sessions',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    tokenHash: text('token_hash').notNull(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
    lastSeenAt: timestamp('last_seen_at', { withTimezone: true }).notNull().defaultNow(),
    expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
    absoluteExpiresAt: timestamp('absolute_expires_at', { withTimezone: true }).notNull(),
    revokedAt: timestamp('revoked_at', { withTimezone: true }),
    ip: inet('ip'),
    userAgent: text('user_agent'),
  },
  (t) => [
    uniqueIndex('sessions_token_hash_key').on(t.tokenHash),
    index('sessions_user_idx').on(t.tenantId, t.userId),
    tenantIsolation(),
  ],
);
