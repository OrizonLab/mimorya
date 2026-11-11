#!/usr/bin/env bash
# The runner cargo is given while the CI lists the tests (ADR-199): instead
# of running a test binary, it records how cargo would run it, its directory
# and the variables cargo sets for it, and the tests it holds, so that
# another job runs it exactly so (run-tests.sh), once the tests are built
# once for all of them.
#
#   CI_TESTS_DIR   where the records go
#   CI_PARENT_ENV  the environment cargo was started with (`env -0`)
set -euo pipefail

binary=$1
out=${CI_TESTS_DIR:?CI_TESTS_DIR: where the records go}
parent=${CI_PARENT_ENV:?CI_PARENT_ENV: the environment cargo was started with}
name=$(basename "$binary")
mkdir -p "$out/runs"

# What cargo set or changed, the shell's own bookkeeping aside.
comm -z -13 <(sort -z "$parent") <(env -0 | sort -z) |
  grep -zv -E '^(PWD|OLDPWD|SHLVL|_)=' > "$out/runs/$name.env" || true

# Each binary once: cargo runs them one after another.
printf '%s\t%s\n' "$binary" "$PWD" >> "$out/runs.tsv"
"$binary" --list --format terse < /dev/null > "$out/runs/$name.all"
"$binary" --list --format terse --ignored < /dev/null > "$out/runs/$name.ignored"
