import { fileURLToPath } from 'node:url';
import { migrate } from 'drizzle-orm/node-postgres/migrator';
import { createDatabase } from './client';

export const MIGRATIONS_FOLDER = fileURLToPath(new URL('../drizzle', import.meta.url));

/** Applies pending migrations. Must run with the owner role, never the app role. */
export async function runMigrations(ownerConnectionString: string): Promise<void> {
  const handle = createDatabase(ownerConnectionString, {
    max: 1,
    applicationName: 'mimorya-migrate',
  });
  try {
    await migrate(handle.db, { migrationsFolder: MIGRATIONS_FOLDER });
  } finally {
    await handle.close();
  }
}
