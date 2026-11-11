// The end-to-end tests as rows (rows.mjs, ADR-199): those Playwright lists
// (`playwright test --list`), or those its `list` reporter shows passed,
// from a shard's output or a former CI job's log. A test is its spec file
// and its titles, never its line, nor its number in the run; the file
// relative to the tests' directory, as `--list` names it (the reporter puts
// `e2e/` before it).
import { clean, line } from './rows.mjs';

const LISTED = /^\s*\[[^\]]+\] › ([^:]+):\d+:\d+ › (.+)$/;
const PASSED = /^\s*✓\s+\d+\s+\[[^\]]+\] › ([^:]+):\d+:\d+ › (.+) \(\d+(?:\.\d+)?(?:ms|s|m)\)$/;

function rows(text, pattern) {
  const found = [];
  for (const raw of text.split('\n')) {
    const entry = clean(raw);
    const match = pattern.exec(entry);
    if (match) found.push(line('e2e', match[1].replace(/^e2e\//, ''), match[2], 'e2e'));
  }
  return found;
}

/** The tests a `playwright test --list` output names, as many as its total says. */
export function listed(text) {
  const found = rows(text, LISTED);
  const total = /^Total: (\d+) tests? in \d+ files?$/m.exec(text.split('\n').map(clean).join('\n'));
  if (!total || Number(total[1]) !== found.length) {
    throw new Error(`${found.length} tests read, not the total Playwright gives`);
  }
  return found;
}

/** The tests a `list` reporter's output shows passed. */
export function passed(text) {
  return rows(text, PASSED);
}
