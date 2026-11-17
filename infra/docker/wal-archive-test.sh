#!/usr/bin/env bash
# Checks the archive of a running production stack's WAL (ADR-206), its
# recipients given (MIMORYA_BACKUP_RECIPIENTS) and the identity that opens
# it at hand (BACKUPS_IDENTITY):
#   - a segment switched is archived within a minute, in the folder of its
#     database system, named by its SHA-256, the file and the folders the
#     database's and readable by it only;
#   - encrypted: another identity cannot open it; the right one decrypts it
#     to a WAL segment of PostgreSQL 16, of 16 MiB, whose SHA-256 is named;
#   - handed over again: with the same contents, the archive command
#     succeeds and adds nothing; with others, it is refused, says so and
#     keeps the first; no partial file is left;
#   - a base backup (backup.sh --base) is readable by its owner only,
#     encrypted, a tar of the data directory with its label and the WAL it
#     needs, and its history is archived beside the segments; by default it
#     goes to backups/, named as a base;
#   - FORMER_WAL_FOLDER, the folder a former run printed, if given: a
#     database made anew archives in another folder, the former one's kept.
# What it adds to the archive, it removes. It reads the archive as root
# (sudo unless root): the folder is the database's (uid 999), its own only.
# The stack's variables and compose files as for production-test.sh. Needs
# age, age-keygen, gzip and tar.
#   BACKUPS_IDENTITY=identity.txt infra/docker/wal-archive-test.sh
set -euo pipefail
identity=${BACKUPS_IDENTITY:-}
[[ -z $identity || $identity == /* ]] || identity=$PWD/$identity
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh

check_name="WAL archive test"
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
[[ -r $identity ]] || fail "BACKUPS_IDENTITY names no identity to read"
command -v age-keygen >/dev/null || fail "age is not installed"
recipient=$(age-keygen -y "$identity") || fail "cannot read the identity's recipient"
archive=${MIMORYA_WAL_ARCHIVE:-./wal-archive}
[[ $archive == /* ]] || archive=$PWD/${archive#./}

as_root() { if ((EUID)); then sudo "$@"; else "$@"; fi; }
sql() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc "$1" || fail "cannot run: $1"
}
# The archived files named <name>.<64 hex digits>.gz.age in the system's folder.
archived() {
  as_root find "$folder" -maxdepth 1 -regextype posix-extended \
    -regex ".*/$1\.[0-9a-f]{64}\.gz\.age" | sort
}
# Waits a minute at most for the file named $1 to be archived; prints it.
await_archived() {
  local found
  for _ in $(seq 1 60); do
    found=$(archived "$1")
    [[ -z $found ]] || {
      echo "$found"
      return
    }
    sleep 1
  done
  fail "$1 was not archived within a minute: $(sql 'select failed_count, last_failed_wal from pg_stat_archiver')"
}
# The archive command run by hand, as the archiver runs it.
archive_again() {
  docker compose exec -T -u postgres -w /var/lib/postgresql/data db \
    bash /usr/local/bin/archive-wal.sh "$@" 2>&1
}

work=$(mktemp -d)
fake=00000001FFFFFFFF000000FF
cleanup() {
  rm -rf "$work"
  [[ -z ${folder:-} ]] || as_root find "$folder" -maxdepth 1 -name "$fake.*" -delete
}
trap cleanup EXIT
age-keygen -o "$work/other.txt" 2>/dev/null

system=$(sql "select system_identifier from pg_control_system()")
folder=$archive/$system
owner=$(docker compose exec -T db id -u postgres) || fail "no postgres user in the database"
sql "select txid_current()" >/dev/null
segment=$(sql "select pg_walfile_name(pg_switch_wal())")
file=$(await_archived "$segment")
for path in "$archive" "$folder"; do
  [[ $(as_root stat -c '%a %u' "$path") == "700 $owner" ]] ||
    fail "$path is not the database's own: $(as_root stat -c '%a %u' "$path")"
done
[[ $(as_root stat -c '%a %u' "$file") == "600 $owner" ]] ||
  fail "$file is not the database's own: $(as_root stat -c '%a %u' "$file")"

if as_root cat "$file" | age --decrypt --identity "$work/other.txt" >/dev/null 2>&1; then
  fail "another identity opens the archive"
fi
as_root cat "$file" | age --decrypt --identity "$identity" | gunzip >"$work/segment" ||
  fail "the archived segment does not decrypt to its end"
[[ $(stat -c %s "$work/segment") == 16777216 ]] || fail "the archived segment is not of 16 MiB"
sum=$(sha256sum <"$work/segment")
[[ $file == *".${sum%% *}.gz.age" ]] || fail "$file is not named by its SHA-256 (${sum%% *})"
# A WAL page of PostgreSQL 16 begins with its magic number, 0xD113.
magic=$(od -An -tx1 -N2 "$work/segment" | tr -d ' \n')
[[ $magic == 13d1 ]] || fail "the archived segment is not of PostgreSQL 16's WAL: $magic"

# Handed over again: the same contents succeed and add nothing; others are
# refused, the first kept. Files of a segment never written stand for it.
docker compose exec -T -u postgres db bash -c \
  'head -c 16777216 /dev/urandom >/tmp/first && head -c 16777216 /dev/urandom >/tmp/second' ||
  fail "cannot write the test's segments"
said=$(archive_again /tmp/first "$fake") || fail "a segment was not archived: $said"
first=$(archived "$fake")
said=$(archive_again /tmp/first "$fake") || fail "the same segment handed over again was refused: $said"
[[ $(archived "$fake") == "$first" ]] || fail "the same segment was archived twice: $(archived "$fake")"
if said=$(archive_again /tmp/second "$fake"); then
  fail "a segment's other contents were archived: $(archived "$fake")"
fi
grep -q 'déjà archivé avec un autre contenu' <<<"$said" ||
  fail "the other contents were not said: $said"
[[ $(archived "$fake") == "$first" ]] || fail "the first contents were not kept: $(archived "$fake")"
docker compose exec -T db rm -f /tmp/first /tmp/second
[[ -z $(as_root find "$folder" -name '.*' -print -quit) ]] ||
  fail "a partial file was left: $(as_root ls -A "$folder" | grep '^\.')"

# A base backup: encrypted, a tar of the data directory with its label and
# its WAL; its history archived beside the segments.
MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh --base "$work/base.tar.gz.age" >/dev/null
[[ $(stat -c %a "$work/base.tar.gz.age") == 600 ]] || fail "the base backup is readable by others"
if age --decrypt --identity "$work/other.txt" "$work/base.tar.gz.age" >/dev/null 2>&1; then
  fail "another identity opens the base backup"
fi
age --decrypt --identity "$identity" "$work/base.tar.gz.age" | gunzip | tar -x -C "$work" \
  backup_label global/pg_control || fail "the base backup holds no label or control file"
listing=$(age --decrypt --identity "$identity" "$work/base.tar.gz.age" | gunzip | tar -t) ||
  fail "the base backup does not decrypt to its end"
grep -Eq '^pg_wal/[0-9A-F]{24}$' <<<"$listing" || fail "the base backup holds no WAL"
start=$(sed -n 's/^START WAL LOCATION: [0-9A-F]*\/\([0-9A-F]*\) (file \([0-9A-F]*\))$/\2.\1/p' \
  "$work/backup_label")
[[ $start =~ ^[0-9A-F]{24}\.[0-9A-F]+$ ]] || fail "unexpected label: $(cat "$work/backup_label")"
offset=$(printf '%08X' $((16#${start#*.} % (16 * 1024 * 1024))))
await_archived "${start%.*}\.$offset\.backup" >/dev/null

# Without a file named: in backups/ at the repository's root, as a base.
[[ -e backups ]] && fresh=no || fresh=yes
said=$(MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh --base)
default=${said#Sauvegarde écrite : }
[[ $default == "$PWD"/backups/mimorya-*Z.base.tar.gz.age && -f $default ]] ||
  fail "unexpected default base backup: $said"
rm -f "$default"
[[ $fresh == no ]] || rmdir backups

# A database made anew (a dump restored into a new stack, ADR-166) archives
# beside the former one's files: FORMER_WAL_FOLDER, if given, is another
# folder, which keeps them.
if [[ -n ${FORMER_WAL_FOLDER:-} ]]; then
  [[ $FORMER_WAL_FOLDER != "$folder" ]] || fail "the new database archives in the former one's folder"
  [[ -n $(as_root find "$FORMER_WAL_FOLDER" -maxdepth 1 -name '*.gz.age' -print -quit) ]] ||
    fail "the former database's archive is gone: $FORMER_WAL_FOLDER"
fi

echo "WAL archive test passed: $folder"
