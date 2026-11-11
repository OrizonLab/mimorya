// What the build job's records say (build-tests.sh, ADR-199): every test of
// every test binary as a row (rows.mjs), its class from whether it is
// ignored, written to `inventory.tsv`; then which binaries each job runs,
// and the files it needs, written to `plans/`: `unit` runs every binary
// that holds a test not ignored, each `int-<k>` its share of the binaries
// that hold ignored ones (PostgreSQL), shared out by their number of
// ignored tests, the most first, each to the shard that holds the fewest.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { line, say } from './rows.mjs';

/** The test binaries cargo built, by path: the target as cargo names it. */
function builtTargets(dir) {
  const targets = new Map();
  for (const text of readFileSync(`${dir}/cargo-messages.json`, 'utf8').split('\n')) {
    if (!text.startsWith('{')) continue;
    const message = JSON.parse(text);
    if (message.reason !== 'compiler-artifact' || !message.executable) continue;
    if (!message.profile?.test) continue;
    const src = path.relative(path.dirname(message.manifest_path), message.target.src_path);
    const kind = message.target.kind[0];
    targets.set(message.executable, kind === 'test' ? src : `unittests ${src}`);
  }
  return targets;
}

/** The variables of an environment file (NUL-separated). */
function variables(file) {
  const vars = new Map();
  for (const entry of readFileSync(file, 'utf8').split('\0')) {
    const at = entry.indexOf('=');
    if (at > 0) vars.set(entry.slice(0, at), entry.slice(at + 1));
  }
  return vars;
}

/** The tests a `--list --format terse` output names. */
function tests(file) {
  return readFileSync(file, 'utf8')
    .split('\n')
    .filter((text) => text.endsWith(': test'))
    .map((text) => text.slice(0, -': test'.length));
}

/** `file` relative to `root`, which it must be under. */
function inside(root, file) {
  const relative = path.relative(root, file);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error(`${file} is outside the workspace ${root}`);
  }
  return relative;
}

/** The binaries cargo ran, each with its target, package, tests and files. */
function binaries(dir, root) {
  const targets = builtTargets(dir);
  const found = [];
  for (const text of readFileSync(`${dir}/runs.tsv`, 'utf8').split('\n')) {
    if (text === '') continue;
    const [binary, cwd] = text.split('\t');
    const name = path.basename(binary);
    const target = targets.get(binary);
    if (!target) throw new Error(`${binary} ran, but cargo built no such test`);
    targets.delete(binary);
    const env = `${dir}/runs/${name}.env`;
    const vars = variables(env);
    const pkg = vars.get('CARGO_PKG_NAME');
    if (!pkg) throw new Error(`${binary}: no CARGO_PKG_NAME`);
    const programs = [...vars]
      .filter(([key]) => key.startsWith('CARGO_BIN_EXE_'))
      .map(([, value]) => inside(root, value));
    const all = tests(`${dir}/runs/${name}.all`);
    const ignored = new Set(tests(`${dir}/runs/${name}.ignored`));
    found.push({
      binary: inside(root, binary),
      cwd: inside(root, cwd),
      env: inside(root, env),
      pkg,
      target,
      programs,
      unit: all.filter((test) => !ignored.has(test)),
      int: all.filter((test) => ignored.has(test)),
    });
  }
  if (targets.size > 0) throw new Error(`built but never run: ${[...targets.keys()].join(', ')}`);
  return found;
}

/** The binaries of each shard: the most ignored tests first, to the lightest. */
function shares(found, shards) {
  const loads = Array.from({ length: shards }, () => ({ tests: 0, binaries: [] }));
  const heavy = found
    .filter((b) => b.int.length > 0)
    .sort((a, b) => b.int.length - a.int.length || (a.binary < b.binary ? -1 : 1));
  for (const binary of heavy) {
    const lightest = loads.reduce((min, load) => (load.tests < min.tests ? load : min));
    lightest.tests += binary.int.length;
    lightest.binaries.push(binary);
  }
  return loads.map((load) => load.binaries);
}

/** Writes `inventory.tsv` and the plans of `dir`'s tests, `shards` shards. */
export function plan(dir, shards) {
  const root = process.cwd();
  const found = binaries(dir, root);
  const inventory = found.flatMap((b) => [
    ...b.unit.map((test) => line(b.pkg, b.target, test, 'unit')),
    ...b.int.map((test) => line(b.pkg, b.target, test, 'int')),
  ]);
  writeFileSync(`${dir}/inventory.tsv`, inventory.map((row) => `${row}\n`).join(''));
  const jobs = [['unit', found.filter((b) => b.unit.length > 0)]];
  shares(found, shards).forEach((share, k) => jobs.push([`int-${k + 1}`, share]));
  mkdirSync(`${dir}/plans`, { recursive: true });
  for (const [name, share] of jobs) {
    const planFile = `${dir}/plans/${name}.tsv`;
    const rows = share.map((b) => [b.binary, b.cwd, b.env, b.pkg, b.target].join('\t'));
    writeFileSync(planFile, rows.map((row) => `${row}\n`).join(''));
    const files = new Set([inside(root, planFile)]);
    for (const b of share) for (const file of [b.binary, b.env, ...b.programs]) files.add(file);
    writeFileSync(`${dir}/plans/${name}.files`, [...files].map((f) => `${f}\n`).join(''));
    const count = share.reduce((sum, b) => sum + b[name === 'unit' ? 'unit' : 'int'].length, 0);
    say(`${name}: ${share.length} binaries, ${count} tests`);
  }
}
