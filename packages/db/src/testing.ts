/**
 * Helpers for integration tests that need a real PostgreSQL database.
 * The test database is dropped and recreated by `resetTestDatabase()`.
 */
import pg from 'pg';
import { createDatabase, withTenant } from './client';
import { runMigrations } from './migrate';
import { memoryItems, tenants, users } from './schema';
import { seedDemoTenant } from './seed';

export interface TestDatabaseUrls {
  owner: string;
  app: string;
  database: string;
}

export function testDatabaseUrls(): TestDatabaseUrls {
  const owner =
    process.env.TEST_DATABASE_URL ?? 'postgres://mimorya:mimorya@localhost:5432/mimorya_test';
  const app =
    process.env.TEST_DATABASE_APP_URL ??
    'postgres://mimorya_app:mimorya_app@localhost:5432/mimorya_test';
  const database = new URL(owner).pathname.slice(1);
  if (!database.endsWith('_test')) {
    throw new Error(`Refusing to reset "${database}": test database names must end with _test`);
  }
  return { owner, app, database };
}

export const OTHER_TENANT_ID = '00000000-0000-4000-8000-0000000f0001';
export const OTHER_USER_ID = '00000000-0000-4000-8000-0000000f0002';

export async function resetTestDatabase(options: { passwordHash?: string | null } = {}) {
  const urls = testDatabaseUrls();
  const admin = new URL(urls.owner);
  admin.pathname = '/postgres';
  const client = new pg.Client({ connectionString: admin.toString() });
  await client.connect();
  try {
    await client.query(`DROP DATABASE IF EXISTS "${urls.database}" WITH (FORCE)`);
    await client.query(`CREATE DATABASE "${urls.database}"`);
  } finally {
    await client.end();
  }

  await runMigrations(urls.owner);

  const handle = createDatabase(urls.owner, { max: 2 });
  try {
    await seedDemoTenant(handle.db, { passwordHash: options.passwordHash ?? null });
    // A second, unrelated tenant used by isolation tests.
    await withTenant(handle.db, OTHER_TENANT_ID, async (tx) => {
      await tx
        .insert(tenants)
        .values({ id: OTHER_TENANT_ID, slug: 'autre-entreprise', name: 'Autre entreprise' });
      await tx.insert(users).values({
        id: OTHER_USER_ID,
        tenantId: OTHER_TENANT_ID,
        email: 'contact@autre-entreprise.example',
        displayName: 'Contact Autre',
        passwordHash: options.passwordHash ?? null,
      });
      await tx.insert(memoryItems).values({
        tenantId: OTHER_TENANT_ID,
        scope: 'tenant',
        kind: 'fact',
        title: 'Secret commercial',
        content: 'Tarif confidentiel négocié avec le client Alpha : 42 € / heure.',
        source: { type: 'manual' },
      });
    });
  } finally {
    await handle.close();
  }
  return urls;
}
