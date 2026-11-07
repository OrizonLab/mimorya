#!/usr/bin/env bash
# Backs up a running production stack and restores it into a new database
# (ADR-166), as an operator would. A company is opened first, and its audit
# log sealed: the anchor the sealing logged is found by the operator and by
# the company's administrator (ADR-178, ADR-179). The stack is backed up for
# an age key made here; a sign-in written after the backup is sealed, its
# anchor kept too. The backup is checked:
#   - readable by its owner only;
#   - encrypted: another identity cannot open it, the right one decrypts it
#     to its end, a PostgreSQL archive;
#   - never replaced: of two backups to one file at once, one writes it, the
#     other is refused at its hard link; a folder made at its name during
#     the dump does not take it; a dangling symbolic link there is refused
#     before the dump; a folder without hard links is said before the dump,
#     with ln's reason; the longest name it takes is not refused for its
#     probe, of its own template; no partial file nor probe is left;
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
#   - its audit chains as they were at the backup (ADR-180): the anchor kept
#     before it is found, the one sealed after it is not, by the operator as
#     by the administrator;
#   - the migrations, which apply again without change.
#
# It throws the stack's volumes away: for a stack thrown away afterwards
# only (the CI's), hence DISPOSABLE_STACK=yes. The stack's variables and
# compose files as for production-test.sh, a master key among them
# (MIMORYA_MASTER_KEY). Needs age, age-keygen and jq.
#   DISPOSABLE_STACK=yes infra/docker/backup-test.sh [https://localhost]
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh
source infra/docker/audit-chain.sh

check_name="backup test"
base=${1:-https://localhost}
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it throws the stack's database away: set DISPOSABLE_STACK=yes on a throwaway stack only"
command -v age-keygen >/dev/null || fail "age is not installed"
command -v jq >/dev/null || fail "jq is not installed"

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
session=$(open_company "$base" "$slug" 'Entreprise sauvegardée' "$password")
before=$(companies)
guarded=$(guards)
((${guarded%% *} > 0)) || fail "no table is under row level security: $guarded"

# The company's audit log sealed: the anchor the sealing logged is found by
# the operator, and by the company's administrator.
tenant=$(company_id "$slug")
kept=$(sealed_anchor "$tenant")
kept_place=$(cut -d: -f2 <<<"$kept")
said=$(operator_verifies "$kept") || fail "the operator's verification failed: $said"
grep -q "ancre n° $kept_place : retrouvée" <<<"$said" || fail "the anchor was not found: $said"
report=$(company_verifies "$base" "$session" "$kept")
check_report "$report" true held

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

# Written after the backup, then sealed: a sign-in, audited. Its anchor will
# be lost once the backup is restored.
sign_in "$base" "admin@$slug.example" "$password" >/dev/null
later=$(sealed_anchor "$tenant")
(($(cut -d: -f2 <<<"$later") > kept_place)) ||
  fail "nothing was sealed after the backup: $kept, then $later"

# A docker standing for the real one: after a dump it makes the folder
# FOLDER_DURING_DUMP names, if any, then waits DUMP_DELAY seconds, as if both
# happened during the dump. backup.sh run through it says what it said.
real_docker=$(command -v docker)
mkdir "$work/bin"
cat >"$work/bin/docker" <<EOF
#!/usr/bin/env bash
"$real_docker" "\$@" || exit
if [[ \$* == *pg_dump* ]]; then
  [[ -z \${FOLDER_DURING_DUMP:-} ]] || mkdir "\$FOLDER_DURING_DUMP"
  sleep "\${DUMP_DELAY:-0}"
fi
EOF
chmod +x "$work/bin/docker"
slow_backup() {
  PATH=$work/bin:$PATH MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh "$@" 2>&1
}

# Two backups to one file at once, both past the first check: one writes it,
# the hard link refuses the other; the file decrypts to its end.
DUMP_DELAY=3 slow_backup "$work/twice.dump.age" >"$work/first.txt" &
first=$!
DUMP_DELAY=3 slow_backup "$work/twice.dump.age" >"$work/second.txt" &
second=$!
written=0
wait "$first" && written=$((written + 1))
wait "$second" && written=$((written + 1))
said=$(cat "$work/first.txt" "$work/second.txt")
((written == 1)) || fail "two backups to one file, $written written: $said"
grep -q 'pendant celle-ci' <<<"$said" || fail "the second backup was not refused at its link: $said"
age --decrypt --identity "$work/identity.txt" "$work/twice.dump.age" | cat >/dev/null ||
  fail "the backup written by one of two runs does not decrypt to its end"

# A folder made at the backup's name during the dump is refused, empty; a
# dangling symbolic link at that name is in the way.
if said=$(FOLDER_DURING_DUMP=$work/folder.dump.age slow_backup "$work/folder.dump.age"); then
  fail "a backup was linked into a folder made at its name: $said"
fi
grep -q 'pendant celle-ci' <<<"$said" || fail "the folder made at its name was not said: $said"
[[ -z $(ls -A "$work/folder.dump.age") ]] || fail "the folder made at the backup's name holds it"
ln -s "$work/nowhere" "$work/dangling.dump.age"
if said=$(slow_backup "$work/dangling.dump.age"); then
  fail "a backup replaced a dangling symbolic link: $said"
fi
# Refused at the first check, before the dump: not at the link after it.
grep -q 'existe déjà' <<<"$said" && ! grep -q 'pendant celle-ci' <<<"$said" ||
  fail "the dangling symbolic link was not said before the dump: $said"

# A folder that takes no hard links is said before the dump: a ln that always
# fails stands for it, and the dump, made ten seconds long, never runs.
printf '#!/usr/bin/env bash\necho "ln: Operation not permitted" >&2\nexit 1\n' >"$work/bin/ln"
chmod +x "$work/bin/ln"
start=$SECONDS
if said=$(DUMP_DELAY=10 slow_backup "$work/unlinked.dump.age"); then
  fail "a backup was written without a hard link: $said"
fi
rm "$work/bin/ln"
grep -qF 'ln: Operation not permitted. Son dossier doit accepter les liens physiques' <<<"$said" ||
  fail "the missing hard links were not said with ln's reason: $said"
((SECONDS - start < 10)) || fail "the hard link was tried only after the dump"

# The longest name a backup takes (240 bytes, its partial file 255) is not
# refused for its probe: the probe, as long as the partial file, is named
# from its own template, never a partial file's. A ln that notes what it
# links stands for the real one.
real_ln=$(command -v ln)
printf '#!/usr/bin/env bash\necho "$*" >>"%s"\nexec "%s" "$@"\n' "$work/ln.txt" "$real_ln" \
  >"$work/bin/ln"
chmod +x "$work/bin/ln"
long=$work/$(printf 'b%.0s' $(seq 1 240))
said=$(slow_backup "$long") || fail "a backup named with 240 bytes was refused: $said"
rm "$work/bin/ln"
read -r _ partial_name probe_name <"$work/ln.txt"
[[ $probe_name == "$long".probe.???????? && ${#probe_name} == "${#partial_name}" ]] ||
  fail "the probe is not of its own template and the partial file's length: $(cat "$work/ln.txt")"
age --decrypt --identity "$work/identity.txt" "$long" | cat >/dev/null ||
  fail "the backup named with 240 bytes does not decrypt to its end"
if compgen -G "$work/*.partial.*" >/dev/null || compgen -G "$work/*.probe.*" >/dev/null; then
  fail "a partial file or a probe was left: $(ls "$work")"
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
# The audit chains as they were at the backup: the anchor kept before it is
# found, the one sealed after it is not.
said=$(operator_verifies "$kept") || fail "the anchor kept before the backup is lost: $said"
if said=$(operator_verifies "$later"); then
  fail "an anchor sealed after the backup is found once it is restored: $said"
fi
grep -q '1 ancre(s) non retrouvée(s)' <<<"$said" ||
  fail "the anchor sealed after the backup was not said lost: $said"
report=$(company_verifies "$base" "$session" "$kept" "$later")
check_report "$report" false 'held,(differs|missing)'
docker compose run --rm --no-deps migrate >/dev/null || fail "migrations not re-applied"

echo "backup test passed: $base"
