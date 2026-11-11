// The tests a `cargo test` log shows passed, as rows (rows.mjs): a log of
// the build job's doctests, or the log of a former CI job, timestamps and
// colours included, whose passes its steps tell apart (`cargo test
// --workspace`: unit, `cargo test-int`: int). Cargo names a target and its
// binary, never its package: a target is the package's of the library
// whose unit tests ran last (cargo runs a package's targets together, its
// library first), a doctest its crate's. A row the attribution got wrong
// does not match the list of the tests built (ADR-199).
import { clean, line } from './rows.mjs';

const RUNNING = /^\s*Running (.+) \(target\/debug\/deps\/(.+)-[0-9a-f]{16}\)$/;
const DOC_TESTS = /^\s*Doc-tests (\S+)$/;
const PASSED = /^test (.+?)(?: - should panic)? \.\.\. ok$/;
const STEP_UNIT = /^##\[group\]Run cargo test --workspace\b/;
const STEP_INT = /^##\[group\]Run cargo test-int\b/;

/** A crate's package, as the workspace names them (`mimorya_api`: `mimorya-api`). */
function packageOf(crate) {
  return crate.replaceAll('_', '-');
}

/** The rows of the tests `text` shows passed, its passes `cls` unless its steps say. */
export function fromCargoLog(text, cls) {
  const rows = [];
  let pass = cls;
  let pkg = null;
  let where = null;
  let doc = false;
  for (const raw of text.split('\n')) {
    const entry = clean(raw);
    if (STEP_UNIT.test(entry)) pass = 'unit';
    if (STEP_INT.test(entry)) pass = 'int';
    const running = RUNNING.exec(entry);
    if (running) {
      const [, target, crate] = running;
      if (target === 'unittests src/lib.rs') pkg = packageOf(crate);
      if (!pkg) throw new Error(`no package before ${target}`);
      where = target;
      doc = false;
      continue;
    }
    const docTests = DOC_TESTS.exec(entry);
    if (docTests) {
      pkg = packageOf(docTests[1]);
      where = 'doc';
      doc = true;
      continue;
    }
    const passed = PASSED.exec(entry);
    if (passed && where) {
      if (!pass) throw new Error('a test passed outside any pass');
      rows.push(line(pkg, where, passed[1], doc ? 'doc' : pass));
    }
  }
  return rows;
}
