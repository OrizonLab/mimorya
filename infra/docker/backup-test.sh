#!/usr/bin/env bash
# Backs up a running production stack and restores it into a new database
# (ADR-166), as an operator would. A company is opened first; the stack is
# backed up for an age key made here. The backup is checked:
#   - readable by its owner only;
#   - encrypted: another identity cannot open it, the right one decrypts it
#     to its end, a PostgreSQL archive;
#   - never replaced: of two backups to one file at once, one writes it, the
#     other is refused, and no partial file is left;
#   - by default in backups/, at the repository's root, named by its full
#     path, the folder and the file readable by their owner only.
# The stack's database is then thrown away (`down -v`) and the backup
# restored into the new one. A second restore, into a database no longer
# empty, is refused by the script's guard. The stack started again serves
# what it held:
#   - its companies, kept apart as they were: as many tables under row level
#     security, policies and rights of the application role;
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

# What keeps the companies apart, counted: tables under row level security,
# their policies, the application role's rights on tables and on functions.
guards() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc \
    "select (select count(*) from pg_class
              where relnamespace = 'public'::regnamespace and relrowsecurity)
         || ' ' || (select count(*) from pg_policies where schemaname = 'public')
         || ' ' || (select count(*) from pg_class cross join aclexplode(relacl) a
                     where relnamespace = 'public'::regnamespace
                       and a.grantee = 'mimorya_app'::regrole)
         || ' ' || (select count(*) from pg_proc cross join aclexplode(proacl) a
                     where pronamespace = 'public'::regnamespace
                       and a.grantee = 'mimorya_app'::regrole)" ||
    fail "cannot count what keeps the companies apart"
}

wait_for_stack "$base"
slug=backup-$(date +%s)
password=$(openssl rand -hex 24)
open_company "$base" "$slug" 'Entreprise sauvegardée' "$password" >/dev/null
before=$(companies)
guarded=$(guards)
((${guarded%% *} > 0)) || fail "no table is under row level security: $guarded"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
age-keygen -o "$work/identity.txt" 2>/dev/null
age-keygen -o "$work/other.txt" 2>/dev/null
recipient=$(age-keygen -y "$work/identity.txt")
MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh "$work/stack.dump.age" >/dev/null
[[ $(stat -c %a "$work/stack.dump.age") == 600 ]] || fail "the backup is readable by others"
if age --decrypt --identity "$work/other.txt" "$work/stack.dump.age" >/dev/null 2>&1; then
  fail "another identity opens the backup"
fi
# Read to its end: age authenticates every chunk, so a damaged backup fails
# here; and a reader that stopped after the first bytes would end age with
# SIGPIPE, which pipefail makes the test's failure.
magic=$(age --decrypt --identity "$work/identity.txt" "$work/stack.dump.age" |
  { head -c 5; cat >/dev/null; }) || fail "the backup does not decrypt to its end"
[[ $magic == PGDMP ]] || fail "the backup does not hold a PostgreSQL archive"

# Two backups to one file at once: one writes it, the other is refused; the
# file decrypts to its end and no partial file is left.
MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh "$work/twice.dump.age" \
  >"$work/first.txt" 2>&1 &
first=$!
MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh "$work/twice.dump.age" \
  >"$work/second.txt" 2>&1 &
second=$!
written=0
wait "$first" && written=$((written + 1))
wait "$second" && written=$((written + 1))
said=$(cat "$work/first.txt" "$work/second.txt")
((written == 1)) || fail "two backups to one file, $written written: $said"
grep -q 'existe déjà' <<<"$said" || fail "the second backup failed otherwise: $said"
age --decrypt --identity "$work/identity.txt" "$work/twice.dump.age" | cat >/dev/null ||
  fail "the backup written by one of two runs does not decrypt to its end"
if compgen -G "$work/*.partial.*" >/dev/null; then
  fail "a partial file was left: $(ls "$work")"
fi

# Without a file named, the backup goes to backups/ in the repository's root,
# named by its full path, readable by its owner only; it is removed here.
[[ -e backups ]] && fresh=no || fresh=yes
said=$(MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh)
default=${said#Sauvegarde écrite : }
[[ $default == "$PWD"/backups/mimorya-*Z.dump.age && -f $default ]] ||
  fail "unexpected default backup: $said"
[[ $(stat -c %a "$default") == 600 ]] || fail "the default backup is readable by others"
if [[ $fresh == yes ]]; then
  [[ $(stat -c %a backups) == 700 ]] || fail "the backups folder is readable by others"
  rm -f "$default" && rmdir backups
else
  rm -f "$default"
fi

docker compose down -v >/dev/null 2>&1 || fail "the stack did not stop"
docker compose up -d db >/dev/null 2>&1 || fail "the new database did not start"
infra/backup/restore.sh "$work/stack.dump.age" "$work/identity.txt" >/dev/null
# Refused by the script's guard: pg_restore would also fail on the objects
# already there, which would not show the guard works.
if refusal=$(infra/backup/restore.sh "$work/stack.dump.age" "$work/identity.txt" 2>&1); then
  fail "a backup was restored into a database that is not empty"
fi
grep -q 'la base contient déjà [0-9]* tables' <<<"$refusal" ||
  fail "the second restore failed, but not on the guard: $refusal"
docker compose up -d >/dev/null 2>&1 || fail "the stack did not start again"

wait_for_stack "$base"
check_headers_and_health "$base"
after=$(companies)
[[ $after == "$before" ]] || fail "companies before ($before) and after ($after) differ"
restored=$(guards)
[[ $restored == "$guarded" ]] ||
  fail "row level security and rights before ($guarded) and after ($restored) differ"
session=$(sign_in "$base" "admin@$slug.example" "$password")
check_company_of "$base" "$session" 'Entreprise sauvegardée'
docker compose run --rm --no-deps migrate >/dev/null || fail "migrations not re-applied"

echo "backup test passed: $base"
