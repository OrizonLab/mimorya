// Fails when the Drizzle schema has changes that no migration captures.
// Works on a temporary copy of the migrations folder (removed afterwards); the
// committed migrations are never modified.
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageDir = join(dirname(fileURLToPath(import.meta.url)), '..');
// drizzle-kit only accepts an output path relative to the package.
const tmp = mkdtempSync(join(packageDir, '.drift-'));
try {
  cpSync(join(packageDir, 'drizzle'), tmp, { recursive: true });
  const before = new Set(readdirSync(tmp));
  const run = spawnSync(
    'pnpm',
    [
      'exec',
      'drizzle-kit',
      'generate',
      '--dialect=postgresql',
      '--schema=./src/schema/index.ts',
      `--out=./${basename(tmp)}`,
      '--name=drift',
    ],
    { cwd: packageDir, encoding: 'utf8' },
  );
  const output = `${run.stdout}${run.stderr}`;
  const added = readdirSync(tmp).filter((f) => !before.has(f) && f.endsWith('.sql'));
  if (added.length > 0) {
    console.error('Schema drift: the Drizzle schema has changes without a migration.');
    console.error('Run `pnpm db:generate`, review the SQL and commit it.');
    process.exit(1);
  }
  if (run.status !== 0 || !/No schema changes/i.test(output)) {
    console.error('Drift check could not run drizzle-kit:\n' + output);
    process.exit(2);
  }
  console.log('No schema drift: migrations match the Drizzle schema.');
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
