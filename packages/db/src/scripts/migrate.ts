import { runMigrations } from '../migrate';

const url = process.env.DATABASE_URL;
if (!url) {
  console.error('DATABASE_URL (owner role) is required to run migrations');
  process.exit(1);
}

await runMigrations(url);
console.log('Migrations applied');
