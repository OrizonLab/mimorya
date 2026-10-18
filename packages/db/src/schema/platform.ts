/** Connectors, encrypted secrets, audit trail and usage metering. */
import type { Permission, ToolRisk } from '@mimorya/core';
import { sql } from 'drizzle-orm';
import {
  bigint,
  bigserial,
  boolean,
  check,
  customType,
  index,
  inet,
  integer,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from 'drizzle-orm/pg-core';
import { oneOf, tenantIsolation, timestamps } from './common';
import { tenants, users } from './tenancy';
import { agentRuns } from './work';

const bytea = customType<{ data: Buffer }>({ dataType: () => 'bytea' });

export const secrets = pgTable(
  'secrets',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    name: text('name').notNull(),
    ciphertext: bytea('ciphertext').notNull(),
    iv: bytea('iv').notNull(),
    authTag: bytea('auth_tag').notNull(),
    keyVersion: integer('key_version').notNull().default(1),
    ...timestamps,
  },
  (t) => [uniqueIndex('secrets_tenant_name_key').on(t.tenantId, t.name), tenantIsolation()],
);

export interface ToolPolicyOverride {
  enabled?: boolean;
  permissions?: Permission[];
  risk?: ToolRisk;
  requiresApproval?: boolean;
}

export const connectorInstances = pgTable(
  'connector_instances',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    type: text('type').notNull(),
    name: text('name').notNull(),
    enabled: boolean('enabled').notNull().default(true),
    config: jsonb('config').$type<Record<string, unknown>>().notNull().default({}),
    toolPolicies: jsonb('tool_policies')
      .$type<Record<string, ToolPolicyOverride>>()
      .notNull()
      .default({}),
    secretId: uuid('secret_id').references(() => secrets.id, { onDelete: 'set null' }),
    status: text('status').notNull().default('ok'),
    lastError: text('last_error'),
    ...timestamps,
  },
  (t) => [
    uniqueIndex('connector_instances_tenant_name_key').on(t.tenantId, t.name),
    check('connector_instances_status_check', oneOf(sql`${t.status}`, ['ok', 'error', 'disabled'])),
    tenantIsolation(),
  ],
);

export const auditLog = pgTable(
  'audit_log',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    occurredAt: timestamp('occurred_at', { withTimezone: true }).notNull().defaultNow(),
    actorType: text('actor_type').notNull(),
    actorUserId: uuid('actor_user_id').references(() => users.id, { onDelete: 'set null' }),
    onBehalfOfUserId: uuid('on_behalf_of_user_id').references(() => users.id, {
      onDelete: 'set null',
    }),
    runId: uuid('run_id').references(() => agentRuns.id, { onDelete: 'set null' }),
    action: text('action').notNull(),
    resourceType: text('resource_type'),
    resourceId: text('resource_id'),
    outcome: text('outcome').notNull(),
    reason: text('reason'),
    metadata: jsonb('metadata').$type<Record<string, unknown>>().notNull().default({}),
    ip: inet('ip'),
    requestId: text('request_id'),
  },
  (t) => [
    index('audit_log_tenant_time_idx').on(t.tenantId, t.occurredAt),
    index('audit_log_run_idx').on(t.runId),
    check('audit_log_actor_check', oneOf(sql`${t.actorType}`, ['user', 'agent', 'system'])),
    check('audit_log_outcome_check', oneOf(sql`${t.outcome}`, ['success', 'denied', 'failure'])),
    tenantIsolation(),
  ],
);

export const usageRecords = pgTable(
  'usage_records',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    tenantId: uuid('tenant_id')
      .notNull()
      .references(() => tenants.id, { onDelete: 'cascade' }),
    userId: uuid('user_id').references(() => users.id, { onDelete: 'set null' }),
    runId: uuid('run_id').references(() => agentRuns.id, { onDelete: 'set null' }),
    kind: text('kind').notNull(),
    provider: text('provider').notNull(),
    model: text('model').notNull(),
    inputTokens: integer('input_tokens').notNull().default(0),
    outputTokens: integer('output_tokens').notNull().default(0),
    costMicros: bigint('cost_micros', { mode: 'number' }).notNull().default(0),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('usage_records_tenant_time_idx').on(t.tenantId, t.createdAt),
    check('usage_records_kind_check', oneOf(sql`${t.kind}`, ['llm', 'embedding', 'tool'])),
    tenantIsolation(),
  ],
);
