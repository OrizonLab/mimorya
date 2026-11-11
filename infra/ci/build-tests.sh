#!/usr/bin/env bash
# The workspace's tests built once for the whole CI (ADR-199), as
# `cargo test --workspace` builds them; the doctests run, ignored ones
# included; each test binary listed with how cargo runs it (record-test.sh,
# cargo's runner here); then the binaries packed for the jobs that run them
# (tests.mjs plan): one archive for the unit tests, one for each shard of
# the PostgreSQL tests. Run from the workspace's root.
#
#   build-tests.sh <shards>   writes target/ci/
set -euo pipefail

shards=${1:?the number of shards of the PostgreSQL tests}
out=target/ci
rm -rf "$out"
mkdir -p "$out/archives"

cargo test --workspace --no-run --message-format=json-render-diagnostics > "$out/cargo-messages.json"

# The doctests run here, ignored ones included: the two passes of the
# former CI in one (ADR-199).
cargo test --workspace --doc -- --include-ignored 2>&1 | tee "$out/doctests.log"
node infra/ci/tests.mjs from-cargo-log --class doc "$out/doctests.log" > "$out/ran-doc.tsv"

# The test binaries cargo runs, its runner recording each instead
# (`tests.mjs plan` checks they are the ones built above).
(
  export CI_TESTS_DIR=$PWD/$out
  export CI_PARENT_ENV=$PWD/$out/parent.env
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=$PWD/infra/ci/record-test.sh
  env -0 > "$CI_PARENT_ENV"
  cargo test --workspace --lib --bins --tests --no-fail-fast
)

node infra/ci/tests.mjs plan "$out" "$shards"

# Many binaries hold mostly the same code: a long window finds it again.
pack() {
  local list=$1
  local name
  name=$(basename "$list" .files)
  tar -cf - -T "$list" | zstd -q -3 --long=30 -o "$out/archives/$name.tar.zst"
}
export -f pack
export out
find "$out/plans" -name '*.files' -print0 | xargs -0 -P 4 -I {} bash -c 'pack "$1"' _ {}
ls -l "$out/archives"
