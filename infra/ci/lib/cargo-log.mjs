// The tests a `cargo test` log shows passed, as rows (rows.mjs): the build
// job's doctests, or a former CI job's log, timestamps and colours
// included, whose passes its steps tell apart (`cargo test --workspace`:
// unit, `cargo test-int`: int). Cargo names each target on its standard
// error; the target's tests write theirs on the standard output, from
// `running N tests` to `test result:`. A log that holds both may set them
// out of order, so the k-th of these sections is the k-th target's (each
// stream keeps its own order). Cargo never names a target's package: it is
// that of the library whose unit tests it named last (it runs a package's
// targets together, its library first), a doctest its crate's. A row the
// attribution got wrong does not match the list of the tests built
// (ADR-199).
import { clean, line } from './rows.mjs';

const RUNNING = /^\s*Running (.+) \(target\/debug\/deps\/(.+)-[0-9a-f]{16}\)$/;
const DOC_TESTS = /^\s*Doc-tests (\S+)$/;
const SECTION = /^running \d+ tests?$/;
const PASSED = /^test (.+?)(?: - should panic)? \.\.\. ok$/;
const RESULT = /^test result: /;
const STEP_UNIT = /^##\[group\]Run cargo test --workspace\b/;
const STEP_INT = /^##\[group\]Run cargo test-int\b/;

/** A crate's package, as the workspace names them (`mimorya_api`: `mimorya-api`). */
function packageOf(crate) {
  return crate.replaceAll('_', '-');
}

/** The rows of the tests `text` shows passed, its passes `cls` unless its steps say. */
export function fromCargoLog(text, cls) {
  const rows = [];
  const targets = [];
  let next = 0;
  let pass = cls;
  let pkg = null;
  let section = null;
  for (const raw of text.split('\n')) {
    const entry = clean(raw);
    if (STEP_UNIT.test(entry)) pass = 'unit';
    if (STEP_INT.test(entry)) pass = 'int';
    const running = RUNNING.exec(entry);
    if (running) {
      const [, where, crate] = running;
      if (where === 'unittests src/lib.rs') pkg = packageOf(crate);
      if (!pkg) throw new Error(`no package before ${where}`);
      if (!pass) throw new Error(`${where} ran outside any pass`);
      targets.push({ pkg, where, cls: pass });
      continue;
    }
    const docTests = DOC_TESTS.exec(entry);
    if (docTests) {
      targets.push({ pkg: packageOf(docTests[1]), where: 'doc', cls: 'doc' });
      continue;
    }
    if (SECTION.test(entry)) {
      section = [];
      continue;
    }
    const passed = PASSED.exec(entry);
    if (passed && section) section.push(passed[1]);
    if (RESULT.test(entry) && section) {
      const target = targets[next++];
      if (!target) throw new Error('a result before any target');
      for (const test of section) rows.push(line(target.pkg, target.where, test, target.cls));
      section = null;
    }
  }
  if (next !== targets.length) throw new Error(`${targets.length - next} targets without a result`);
  return rows;
}
