import type { Condition } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import {
  boolean,
  check,
  index,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps } from './common';
import { orgUnits, tenants, users } from './tenancy';

export const roles = pgTable(
  'roles',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    key: text('key').notNull(),
    name: text('name').notNull(),
    description: text('description').notNull().default(''),
    permissions: text('permissions')
      .array()
      .notNull()
      .default(sql`'{}'::text[]`),
    isSystem: boolean('is_system').notNull().default(false),
    ...timestamps,
  },
  (t) => [uniqueIndex('roles_tenant_key').on(t.tenantId, t.key), tenantIsolation()],
);

export const roleAssignments = pgTable(
  'role_assignments',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    roleId: uuid('role_id')
      .notNull()
      .references(() => roles.id, { onDelete: 'cascade' }),
    scopeType: text('scope_type').notNull(),
    scopeOrgUnitId: uuid('scope_org_unit_id').references(() => orgUnits.id, {
      onDelete: 'cascade',
    }),
    includeDescendants: boolean('include_descendants').notNull().default(true),
    grantedByUserId: uuid('granted_by_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    expiresAt: timestamp('expires_at', { withTimezone: true }),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('role_assignments_user_idx').on(t.tenantId, t.userId),
    check(
      'role_assignments_scope_type_check',
      oneOf(sql`${t.scopeType}`, ['tenant', 'org_unit', 'self']),
    ),
    check(
      'role_assignments_scope_unit_check',
      sql`(${t.scopeType} = 'org_unit') = (${t.scopeOrgUnitId} is not null)`,
    ),
    tenantIsolation(),
  ],
);

export const accessPolicies = pgTable(
  'access_policies',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    description: text('description').notNull(),
    effect: text('effect').notNull(),
    permissions: text('permissions').array().notNull(),
    condition: jsonb('condition').$type<Condition>().notNull(),
    enabled: boolean('enabled').notNull().default(true),
    ...timestamps,
  },
  (t) => [
    index('access_policies_tenant_idx').on(t.tenantId),
    check('access_policies_effect_check', oneOf(sql`${t.effect}`, ['allow', 'deny'])),
    tenantIsolation(),
  ],
);
