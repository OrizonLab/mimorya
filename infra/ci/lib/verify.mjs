// Two lists of tests compared (ADR-199): the tests listed, or those a
// former CI ran, against the ones that ran and passed, each once. Every
// difference is named; their counts and digests are printed, so that two
// runs can be compared at a glance.
import { digest, say, sorted } from './rows.mjs';

/** The rows `rows` holds more than once. */
function twice(rows) {
  const seen = new Set();
  const again = new Set();
  for (const row of rows) (seen.has(row) ? again : seen).add(row);
  return sorted(again);
}

/** Whether `ran` is `expected`, each row once; what differs is printed. */
export function verify(expected, ran, names = ['expected', 'ran']) {
  const [left, right] = names;
  for (const [name, rows] of [
    [left, expected],
    [right, ran],
  ]) {
    const { count, sha256 } = digest(rows);
    say(`${name}: ${count} tests, sha256 ${sha256}`);
  }
  if (expected.length === 0) {
    say(`${left}: no test listed`);
    return false;
  }
  const expectedSet = new Set(expected);
  const ranSet = new Set(ran);
  const problems = [
    ...twice(expected).map((row) => `listed twice in ${left}: ${row}`),
    ...twice(ran).map((row) => `twice in ${right}: ${row}`),
    ...sorted([...expectedSet].filter((row) => !ranSet.has(row))).map(
      (row) => `in ${left}, not in ${right}: ${row}`,
    ),
    ...sorted([...ranSet].filter((row) => !expectedSet.has(row))).map(
      (row) => `in ${right}, not in ${left}: ${row}`,
    ),
  ];
  for (const problem of problems) say(problem);
  say(problems.length === 0 ? 'identical' : `${problems.length} differences`);
  return problems.length === 0;
}
