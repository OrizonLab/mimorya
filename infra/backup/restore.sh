#!/usr/bin/env bash
# Restores a backup of backup.sh (ADR-166) into the database of a new stack,
# still empty: its database service alone started (`docker compose up -d
# db`), before the migrations; a database that holds tables is refused. The
# identity (the age private key) opens the backup on the way in, and the
# dump is read in one transaction: all or nothing. Then `up -d` starts the
# stack, whose migrations apply what the backup lacks.
#
# The new stack keeps the backed-up stack's MIMORYA_MASTER_KEY: the secrets
# the database holds are sealed with it. Its database passwords may differ:
# roles are not part of a backup, the new stack's own are kept.
#   infra/backup/restore.sh backups/mimorya-….dump.age identity.txt
# From the repository's root, where it runs from wherever started; the
# stack's variables and files as for backup.sh. Needs age on the host.
set -euo pipefail
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

fail() {
  echo "restore failed: $1" >&2
  exit 1
}

(($# == 2)) || fail "usage: infra/backup/restore.sh <backup file> <age identity file>"
file=$1
identity=$2
command -v age >/dev/null || fail "age is not installed (https://age-encryption.org)"
[[ -r $file ]] || fail "cannot read $file"
[[ -r $identity ]] || fail "cannot read $identity"

# The database is ready once it listens over TCP: while it initialises (the
# application's role, the extensions), it answers on its socket only.
ready=no
for _ in $(seq 1 60); do
  if docker compose exec -T db pg_isready --host 127.0.0.1 --username mimorya >/dev/null 2>&1; then
    ready=yes
    break
  fi
  sleep 2
done
[[ $ready == yes ]] || fail "the stack's database is not ready (docker compose up -d db)"
tables=$(docker compose exec -T db psql --username mimorya --dbname mimorya -tAc \
  "select count(*) from pg_tables where schemaname = 'public'") ||
  fail "cannot read the stack's database"
[[ $tables == 0 ]] ||
  fail "the database holds $tables tables: restore into a new stack's database only"

age --decrypt --identity "$identity" "$file" |
  docker compose exec -T db pg_restore --username mimorya --dbname mimorya \
    --single-transaction --exit-on-error ||
  fail "nothing was restored (wrong identity, damaged file, or a database that refused it)"
echo "backup restored: $file; start the stack now (up -d)"
