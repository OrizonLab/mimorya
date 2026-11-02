#!/usr/bin/env bash
# Backs up a running production stack and restores it into a new database
# (ADR-166), as an operator would. A company is opened first; the stack is
# backed up for an age key made here. The backup is checked:
#   - readable by its owner only;
#   - encrypted: another identity cannot open it, the right one opens a
#     PostgreSQL archive.
# The stack's database is then thrown away (`down -v`) and the backup
# restored into the new one. A second restore, into a database no longer
# empty, is refused. The stack started again serves what it held:
#   - its companies;
#   - the company's administrator, who signs in with the password chosen
#     before the backup;
#   - the migrations, which apply again without change.
#
# It throws the stack's volumes away: for a stack thrown away afterwards
# only (the CI's), hence DISPOSABLE_STACK=yes. The stack's variables and
# compose files as for production-test.sh. Needs age and age-keygen.
#   DISPOSABLE_STACK=yes infra/docker/backup-test.sh [https://localhost]
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh

check_name="backup test"
base=${1:-https://localhost}
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it throws the stack's database away: set DISPOSABLE_STACK=yes on a throwaway stack only"
command -v age-keygen >/dev/null || fail "age is not installed"

companies() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc \
    "select string_agg(slug, ',' order by slug) from tenants" ||
    fail "cannot list the companies"
}

wait_for_stack "$base"
slug=backup-$(date +%s)
password=$(openssl rand -hex 24)
open_company "$base" "$slug" 'Entreprise sauvegardée' "$password" >/dev/null
before=$(companies)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
age-keygen -o "$work/identity.txt" 2>/dev/null
age-keygen -o "$work/other.txt" 2>/dev/null
MIMORYA_BACKUP_RECIPIENTS=$(age-keygen -y "$work/identity.txt") \
  infra/backup/backup.sh "$work/stack.dump.age" >/dev/null
[[ $(stat -c %a "$work/stack.dump.age") == 600 ]] || fail "the backup is readable by others"
if age --decrypt --identity "$work/other.txt" "$work/stack.dump.age" >/dev/null 2>&1; then
  fail "another identity opens the backup"
fi
magic=$(age --decrypt --identity "$work/identity.txt" "$work/stack.dump.age" | head -c 5)
[[ $magic == PGDMP ]] || fail "the backup does not hold a PostgreSQL archive"

docker compose down -v >/dev/null 2>&1 || fail "the stack did not stop"
docker compose up -d db >/dev/null 2>&1 || fail "the new database did not start"
infra/backup/restore.sh "$work/stack.dump.age" "$work/identity.txt" >/dev/null
if infra/backup/restore.sh "$work/stack.dump.age" "$work/identity.txt" >/dev/null 2>&1; then
  fail "a backup was restored into a database that is not empty"
fi
docker compose up -d >/dev/null 2>&1 || fail "the stack did not start again"

wait_for_stack "$base"
check_headers_and_health "$base"
after=$(companies)
[[ $after == "$before" ]] || fail "companies before ($before) and after ($after) differ"
session=$(sign_in "$base" "admin@$slug.example" "$password")
check_company_of "$base" "$session" 'Entreprise sauvegardée'
docker compose run --rm --no-deps migrate >/dev/null || fail "migrations not re-applied"

echo "backup test passed: $base"
