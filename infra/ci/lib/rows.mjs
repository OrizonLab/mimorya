// The CI's rows of tests (ADR-199), one per test, tab-separated:
//   suite  where  test  class
// - Rust: the package, the target as cargo names it (`unittests src/lib.rs`,
//   `tests/auth.rs`, `doc`), the test, and the pass that runs it: `unit`
//   (`cargo test --workspace`), `int` (ignored, PostgreSQL), `doc`;
// - end to end: `e2e`, the spec file, the test's titles, `e2e`.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const TIMESTAMP = /^\d{4}-\d\d-\d\dT[\d:.]+Z /;
// Built from its code: a control character may not appear in a pattern.
const COLOUR = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, 'g');

/** A log's line as written: without the timestamp GitHub puts before it, nor colours. */
export function clean(raw) {
  return raw.replace(TIMESTAMP, '').replace(COLOUR, '').replace(/\r$/, '');
}

/** Writes `text` and a line break on the standard output. */
export function say(text) {
  process.stdout.write(`${text}\n`);
}

/** A row's line. */
export function line(suite, where, test, cls) {
  for (const field of [suite, where, test, cls]) {
    if (!field || /[\t\n]/.test(field)) throw new Error(`unfit field: ${JSON.stringify(field)}`);
  }
  return `${suite}\t${where}\t${test}\t${cls}`;
}

/** The rows of files, each line once or more. */
export function readRows(files) {
  const rows = [];
  for (const file of files) {
    for (const text of readFileSync(file, 'utf8').split('\n')) {
      if (text.trim() !== '') rows.push(text);
    }
  }
  return rows;
}

/** Sorted rows, the way every list is compared and hashed. */
export function sorted(rows) {
  return [...rows].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/** How many rows, and the SHA-256 of their sorted lines. */
export function digest(rows) {
  const text = sorted(rows)
    .map((row) => `${row}\n`)
    .join('');
  return { count: rows.length, sha256: createHash('sha256').update(text).digest('hex') };
}
