import { DEMO_TENANT, DEMO_USERS } from '@mimorya/core/demo';
import { sql } from 'drizzle-orm';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createDatabase, withTenant, type DatabaseHandle } from './client';
import { auditLog, memoryItems, users } from './schema';
import { OTHER_TENANT_ID, resetTestDatabase } from './testing';

let app: DatabaseHandle;

/** Drizzle wraps driver errors ("Failed query: …"); the PostgreSQL message is the cause. */
async function expectDbError(promise: Promise<unknown>, pattern: RegExp) {
  const error = await promise.then(
    () => undefined,
    (e: unknown) => e,
  );
  expect(error, 'expected the query to fail').toBeInstanceOf(Error);
  const cause = (error as Error & { cause?: unknown }).cause;
  const message = cause instanceof Error ? cause.message : (error as Error).message;
  expect(message).toMatch(pattern);
}

beforeAll(async () => {
  const urls = await resetTestDatabase();
  app = createDatabase(urls.app, { max: 4 });
});

afterAll(async () => {
  await app?.close();
});

describe('tenant isolation (row-level security, application role)', () => {
  it('connects with a role that cannot bypass RLS', async () => {
    const result = await app.db.execute<{ rolsuper: boolean; rolbypassrls: boolean }>(
      sql`select rolsuper, rolbypassrls from pg_roles where rolname = current_user`,
    );
    expect(result.rows[0]).toEqual({ rolsuper: false, rolbypassrls: false });
  });

  it('sees nothing without a tenant context', async () => {
    const rows = await app.db.select().from(users);
    expect(rows).toHaveLength(0);
    const mem = await app.db.select().from(memoryItems);
    expect(mem).toHaveLength(0);
  });

  it('sees only the rows of the current tenant', async () => {
    const demoUsers = await withTenant(app.db, DEMO_TENANT.id, (tx) => tx.select().from(users));
    expect(demoUsers).toHaveLength(10);
    expect(demoUsers.every((u) => u.tenantId === DEMO_TENANT.id)).toBe(true);

    const demoMemory = await withTenant(app.db, DEMO_TENANT.id, (tx) =>
      tx.select().from(memoryItems),
    );
    expect(demoMemory.some((m) => m.content.includes('Tarif confidentiel'))).toBe(false);

    const otherUsers = await withTenant(app.db, OTHER_TENANT_ID, (tx) => tx.select().from(users));
    expect(otherUsers.map((u) => u.email)).toEqual(['contact@autre-entreprise.example']);
  });

  it('ignores explicit filters targeting another tenant', async () => {
    const rows = await withTenant(app.db, DEMO_TENANT.id, (tx) =>
      tx.execute(sql`select * from memory_items where tenant_id = ${OTHER_TENANT_ID}`),
    );
    expect(rows.rows).toHaveLength(0);
  });

  it('rejects writes into another tenant', async () => {
    await expectDbError(
      withTenant(app.db, DEMO_TENANT.id, (tx) =>
        tx.insert(memoryItems).values({
          tenantId: OTHER_TENANT_ID,
          scope: 'tenant',
          kind: 'fact',
          title: 'Injection',
          content: 'tentative',
          source: { type: 'manual' },
        }),
      ),
      /row-level security/,
    );

    await expectDbError(
      withTenant(app.db, DEMO_TENANT.id, (tx) =>
        tx.execute(
          sql`update users set tenant_id = ${OTHER_TENANT_ID} where id = ${DEMO_USERS.lucas}`,
        ),
      ),
      /row-level security/,
    );
  });

  it('keeps the audit log append-only', async () => {
    await withTenant(app.db, DEMO_TENANT.id, (tx) =>
      tx.insert(auditLog).values({
        tenantId: DEMO_TENANT.id,
        actorType: 'system',
        action: 'test.append',
        outcome: 'success',
      }),
    );
    await expectDbError(
      withTenant(app.db, DEMO_TENANT.id, (tx) => tx.execute(sql`delete from audit_log`)),
      /permission denied/,
    );
    await expectDbError(
      withTenant(app.db, DEMO_TENANT.id, (tx) =>
        tx.execute(sql`update audit_log set action = 'tampered'`),
      ),
      /permission denied/,
    );
  });

  it('protects every tenant-scoped table with RLS and a policy', async () => {
    const result = await app.db.execute<{ table_name: string; rls: boolean; policies: number }>(sql`
      select c.relname as table_name,
             c.relrowsecurity as rls,
             (select count(*)::int from pg_policies p where p.tablename = c.relname) as policies
      from pg_class c
      join pg_namespace n on n.oid = c.relnamespace
      where n.nspname = 'public' and c.relkind = 'r'
    `);
    expect(result.rows.length).toBeGreaterThanOrEqual(24);
    const unprotected = result.rows.filter((r) => !r.rls || r.policies < 1);
    expect(unprotected).toEqual([]);
  });

  it('exposes only the minimal pre-tenant lookups', async () => {
    const login = await app.db.execute<{ user_id: string; tenant_id: string }>(
      sql`select user_id, tenant_id from auth_find_login(${'MARC.LEFEVRE@neo-maintenance.example'})`,
    );
    expect(login.rows).toEqual([{ user_id: DEMO_USERS.marc, tenant_id: DEMO_TENANT.id }]);

    const session = await app.db.execute(sql`select * from auth_resolve_session(${'unknown'})`);
    expect(session.rows).toHaveLength(0);
  });
});
