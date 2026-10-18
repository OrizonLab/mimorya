import { sql, type SQL } from 'drizzle-orm';
import { customType, pgPolicy, timestamp } from 'drizzle-orm/pg-core';

/** Current tenant as set by `withTenant()` (transaction-local). NULL when unset. */
export const CURRENT_TENANT = sql`nullif(current_setting('app.tenant_id', true), '')::uuid`;

/**
 * Row-level security policy applied to every tenant-scoped table: rows are
 * only visible and writable when they belong to the transaction's tenant.
 */
export function tenantIsolation(column: SQL = sql`tenant_id`) {
  const predicate = sql`${column} = ${CURRENT_TENANT}`;
  return pgPolicy('tenant_isolation', {
    as: 'permissive',
    for: 'all',
    using: predicate,
    withCheck: predicate,
  });
}

export const citext = customType<{ data: string }>({
  dataType: () => 'citext',
});

export const tsvector = customType<{ data: string }>({
  dataType: () => 'tsvector',
});

export const timestamps = {
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true })
    .notNull()
    .defaultNow()
    .$onUpdate(() => new Date()),
};

/** SQL CHECK expression constraining a text column to a fixed set of values. */
export function oneOf(column: SQL, values: readonly string[]): SQL {
  return sql`${column} in (${sql.join(
    values.map((v) => sql.raw(`'${v.replace(/'/g, "''")}'`)),
    sql`, `,
  )})`;
}
