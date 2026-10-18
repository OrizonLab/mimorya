/**
 * Seeds the FICTIONAL demo company. Development and demonstration only.
 *   DATABASE_URL   owner connection (migrations role)
 *   DEMO_PASSWORD  password for every demo account (required in production)
 */
import { createDatabase, runMigrations, seedDemoTenant } from '@mimorya/db';
import { hashPassword } from '../auth/credentials';

const url = process.env.DATABASE_URL;
if (!url) {
  console.error('DATABASE_URL (owner role) is required');
  process.exit(1);
}
const password =
  process.env.DEMO_PASSWORD ?? (process.env.NODE_ENV === 'production' ? undefined : 'demo-mimorya');
if (!password) {
  console.error('DEMO_PASSWORD is required in production');
  process.exit(1);
}

await runMigrations(url);
const handle = createDatabase(url, { max: 1 });
try {
  const result = await seedDemoTenant(handle.db, {
    passwordHash: await hashPassword(password),
    reset: process.argv.includes('--reset'),
  });
  console.log(
    result === 'created' ? 'Demo tenant created' : 'Demo tenant already present (use --reset)',
  );
} finally {
  await handle.close();
}
