#!/usr/bin/env bash
# Runs the test binaries of a plan of the build job (tests.mjs plan,
# ADR-199) as `cargo test` runs them: from their package's directory, with
# the variables cargo sets for them, the unit tests as
# `cargo test --workspace` (no argument), the PostgreSQL ones as
# `cargo test-int` (`--ignored --test-threads=1`). Every binary runs, even
# after a failure; the rows of the tests that passed are written for the
# last job's comparison. Run from the workspace's root, where the archive
# was unpacked.
#
#   run-tests.sh <plan.tsv> <unit|int> <rows.tsv>
set -uo pipefail

plan=$1
class=$2
rows=$3
root=$PWD
case "$class" in
  unit) args=() ;;
  int) args=(--ignored --test-threads=1) ;;
  *) echo "class: unit or int, not $class" >&2; exit 2 ;;
esac

mkdir -p "$(dirname "$rows")"
: > "$rows"
failed=0
output=$(mktemp)
while IFS=$'\t' read -r binary cwd env pkg target; do
  mapfile -d '' vars < "$env"
  # The paths built in are absolute: the workspace must not have moved.
  if ! printf '%s\n' "${vars[@]}" | grep -qxF "CARGO_MANIFEST_DIR=$root/$cwd"; then
    echo "$binary was built for another workspace than $root" >&2
    exit 2
  fi
  echo "     Running $target ($binary) [$pkg]"
  (cd "$cwd" && env "${vars[@]}" "$root/$binary" "${args[@]}") < /dev/null 2>&1 | tee "$output"
  status=${PIPESTATUS[0]}
  if [ "$status" -ne 0 ]; then
    echo "::error::$pkg $target: exit status $status"
    failed=1
  fi
  passed=$(sed -nE 's/^test (.+) \.\.\. ok$/\1/p' "$output" | sed -E 's/ - should panic$//')
  if [ -n "$passed" ]; then
    printf '%s\n' "$passed" | while IFS= read -r test; do
      printf '%s\t%s\t%s\t%s\n' "$pkg" "$target" "$test" "$class"
    done >> "$rows"
  fi
  # A line broken by output the test did not capture would go unread: said
  # here, where it happened, rather than only as a test missing at the end.
  said=$(sed -nE 's/^test result: [a-zA-Z]+\. ([0-9]+) passed;.*/\1/p' "$output")
  read_count=$(printf '%s' "$passed" | grep -c '^' || true)
  if [ "${said:-0}" != "$read_count" ]; then
    echo "::warning::$pkg $target: $said tests passed, $read_count read from the output"
  fi
done < "$plan"
rm -f "$output"
exit "$failed"
