#!/usr/bin/env node
// The CI's lists of tests (ADR-199): what the build job's records say, what
// a log shows ran, and the comparison of two lists, which the last job of
// the CI makes so that no test goes unrun when they are shared out among
// jobs. Its rows: lib/rows.mjs.
//
//   tests.mjs plan <dir> <shards>                    inventory and plans of the built tests
//   tests.mjs from-cargo-log [--class C] <log>...    rows of the tests a cargo log shows passed
//   tests.mjs e2e-listed <output>                    rows of `playwright test --list`
//   tests.mjs e2e-passed <output>...                 rows a `list` reporter shows passed
//   tests.mjs verify <expected> -- <ran>...          each expected row ran once, and only them
//   tests.mjs digest <rows>...                       their count and SHA-256, then the rows, sorted
import { readFileSync } from 'node:fs';
import { fromCargoLog } from './lib/cargo-log.mjs';
import { plan } from './lib/plan.mjs';
import { listed, passed } from './lib/playwright-log.mjs';
import { digest, readRows, say, sorted } from './lib/rows.mjs';
import { verify } from './lib/verify.mjs';

function print(rows) {
  process.stdout.write(rows.map((row) => `${row}\n`).join(''));
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case 'plan': {
    const [dir, shards] = args;
    const count = Number(shards);
    if (!dir || !Number.isInteger(count) || count < 1) throw new Error('plan <dir> <shards>');
    plan(dir, count);
    break;
  }
  case 'from-cargo-log': {
    const at = args.indexOf('--class');
    const cls = at >= 0 ? args[at + 1] : undefined;
    const files = at >= 0 ? args.filter((_, i) => i !== at && i !== at + 1) : args;
    print(files.flatMap((file) => fromCargoLog(readFileSync(file, 'utf8'), cls)));
    break;
  }
  case 'e2e-listed':
    print(listed(readFileSync(args[0], 'utf8')));
    break;
  case 'e2e-passed':
    print(args.flatMap((file) => passed(readFileSync(file, 'utf8'))));
    break;
  case 'verify': {
    const at = args.indexOf('--');
    if (at < 1) throw new Error('verify <expected>... -- <ran>...');
    const same = verify(readRows(args.slice(0, at)), readRows(args.slice(at + 1)), [
      args.slice(0, at).join(' + '),
      'ran',
    ]);
    process.exitCode = same ? 0 : 1;
    break;
  }
  case 'digest': {
    const rows = readRows(args);
    const { count, sha256 } = digest(rows);
    say(`${count} tests, sha256 ${sha256}`);
    print(sorted(rows));
    break;
  }
  default:
    throw new Error(`unknown command: ${command ?? '(none)'}`);
}
