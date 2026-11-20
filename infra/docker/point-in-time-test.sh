#!/usr/bin/env bash
# Restores a running production stack to a point in time (ADR-214), as an
# operator would after a mistake: a base backup (backup.sh --base), then the
# archived WAL (ADR-206) replayed up to a chosen instant, into the stack's
# database thrown away. Three companies are opened one after the other, an
# instant noted between each, and the segment that holds them archived.
#   - The restore is refused while the stack's database runs, while it holds
#     files, for an instant that could end its setting, with an identity
#     that does not open the base, and when the archive's folder holds none
#     of this database's.
#   - A damaged archive (a segment whose contents differ from its name)
#     stops the recovery, said with the reason, never ends it short.
#   - Restored to the first instant, the stack serves the first company,
#     whose administrator signs in, and neither of the others. Its database
#     is the same system, on a new timeline, without the recovery's settings
#     nor its container left; the new timeline is archived beside the former
#     one, with its history.
#   - Restored again from the same base and archive to the second instant,
#     on the base's own timeline: the first two companies, not the third, on
#     a timeline beyond both former ones; the archiver has nothing left to
#     archive and has failed none, the two restores' files side by side.
#
# It throws the stack's database away: for a stack thrown away afterwards
# only (the CI's), hence DISPOSABLE_STACK=yes. The identity that opens the
# archive (BACKUPS_IDENTITY), the stack's variables and compose files as for
# wal-archive-test.sh; the archive read as root (archive-checks.sh). Needs
# age, age-keygen and gzip.
#   DISPOSABLE_STACK=yes BACKUPS_IDENTITY=identity.txt infra/docker/point-in-time-test.sh [https://localhost]
set -euo pipefail
identity=${BACKUPS_IDENTITY:-}
[[ -z $identity || $identity == /* ]] || identity=$PWD/$identity
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh
source infra/docker/archive-checks.sh

check_name="point in time test"
base=${1:-https://localhost}
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it throws the stack's database away: set DISPOSABLE_STACK=yes on a throwaway stack only"
[[ -r $identity ]] || fail "BACKUPS_IDENTITY names no identity to read"
command -v age-keygen >/dev/null || fail "age is not installed"
recipient=$(age-keygen -y "$identity") || fail "cannot read the identity's recipient"
archive=${MIMORYA_WAL_ARCHIVE:-./wal-archive}
[[ $archive == /* ]] || archive=$PWD/${archive#./}

sql() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc "$1" || fail "cannot run: $1"
}
# The timeline the database writes, as a number.
timeline() {
  local hex
  hex=$(sql "select substr(pg_walfile_name(pg_current_wal_lsn()), 1, 8)")
  echo $((16#$hex))
}
# Switches to a new segment, something written first; prints the former.
switched() {
  sql "select txid_current()" >/dev/null
  sql "select pg_walfile_name(pg_switch_wal())"
}
# The companies this test opened, by slug.
opened() {
  sql "select coalesce(string_agg(slug, ',' order by slug), '') from tenants where slug like '$stamp-%'"
}
# An instant between two commits: after the former, before the next.
noted_instant() {
  sql "select clock_timestamp()"
  sleep 1
}
restore() {
  infra/backup/restore-base.sh "$work/base.tar.gz.age" "$@" 2>&1
}
# Fails unless the restore with the identity and instant given is refused,
# saying $1; what it said is kept in `refusal`.
refused() {
  local reason=$1
  shift
  if refusal=$(restore "$@"); then
    fail "a restore was not refused ($reason): $refusal"
  fi
  grep -qF "$reason" <<<"$refusal" || fail "the restore was refused, but not for $reason: $refusal"
}
throw_away() {
  docker compose down -v >/dev/null 2>&1 || fail "the stack did not stop"
}
start_restored() {
  docker compose up -d >/dev/null 2>&1 || fail "the restored stack did not start"
  wait_for_stack "$base"
}

work=$(mktemp -d)
trap 'as_root rm -rf "$work"' EXIT
age-keygen -o "$work/other.txt" 2>/dev/null

wait_for_stack "$base"
system=$(sql "select system_identifier from pg_control_system()")
folder=$archive/$system
owner=$(as_root stat -c %u "$folder") || fail "the stack archives nothing in $folder"
before=$(timeline)
MIMORYA_BACKUP_RECIPIENTS=$recipient infra/backup/backup.sh --base "$work/base.tar.gz.age" >/dev/null

stamp=instant-$(date +%s)
password=$(openssl rand -hex 24)
open_company "$base" "$stamp-a" 'Entreprise A' "$password" >/dev/null
first_instant=$(noted_instant)
open_company "$base" "$stamp-b" 'Entreprise B' "$password" >/dev/null
second_instant=$(noted_instant)
open_company "$base" "$stamp-c" 'Entreprise C' "$password" >/dev/null
[[ $(opened) == "$stamp-a,$stamp-b,$stamp-c" ]] || fail "the three companies were not opened: $(opened)"
segment=$(switched)
await_archived "$segment" >/dev/null

# The refusals, each before anything is changed or with the database left
# to throw away, as the script says.
refused 'la base de la pile est démarrée' "$identity" "$first_instant"
refused 'instant illisible' "$identity" "$first_instant'; select 1"
docker compose stop >/dev/null 2>&1 || fail "the stack did not stop"
refused 'la base contient déjà des fichiers' "$identity" "$first_instant"
throw_away
refused "rien n'a été restauré (mauvaise identité" "$work/other.txt" "$first_instant"
throw_away
mkdir "$work/elsewhere"
MIMORYA_WAL_ARCHIVE=$work/elsewhere refused "l'archive des WAL de cette base ($system) n'est pas" \
  "$identity" "$first_instant"
throw_away

# A copy of the archive whose first segment the recovery reads, the base's
# own (named by its history, the last archived), holds other contents,
# encrypted for the same recipient: the recovery stops on it, and says why.
start=$(archived '[0-9A-F]{24}\.[0-9A-F]{8}\.backup' | tail -n 1)
start=${start##*/}
start=${start:0:24}
file=$(archived "$start")
[[ -n $file ]] || fail "the base's first segment, $start, is not archived"
damaged=$work/damaged
as_root mkdir -m 700 "$damaged"
as_root cp -a "$folder" "$damaged/"
head -c 16777216 /dev/zero | gzip | age --recipient "$recipient" >"$work/zeros.gz.age"
as_root install -m 600 -o "$owner" -g "$owner" "$work/zeros.gz.age" "$damaged/$system/${file##*/}"
MIMORYA_WAL_ARCHIVE=$damaged refused "la récupération s'est arrêtée" "$identity" "$second_instant"
grep -qF "Restauration arrêtée : le WAL $start de l'archive est inutilisable (son contenu ne correspond pas à son nom)" \
  <<<"$refusal" || fail "the damaged segment was not said: $refusal"
throw_away

# Restored to the first instant.
said=$(restore "$identity" "$first_instant") || fail "the restore to the first instant failed: $said"
grep -qF "Base restaurée jusqu'à $first_instant, ligne temporelle" <<<"$said" ||
  fail "unexpected restore: $said"
start_restored
check_headers_and_health "$base"
[[ $(opened) == "$stamp-a" ]] || fail "restored to the first instant, the companies are: $(opened)"
session=$(sign_in "$base" "admin@$stamp-a.example" "$password")
check_company_of "$base" "$session" 'Entreprise A'
[[ $(sql "select system_identifier from pg_control_system()") == "$system" ]] ||
  fail "the restored database is another system"
first=$(timeline)
((first > before)) || fail "the restored database writes timeline $first, not beyond $before"
[[ -z $(sql 'show restore_command') && -z $(sql 'show recovery_target_time') ]] ||
  fail "the recovery's settings were kept"
[[ -z $(docker ps -aq --filter name=mimorya-restore-) ]] || fail "the recovery's container was left"
hex=$(printf '%08X' "$first")
segment=$(switched)
[[ $segment == "$hex"* ]] || fail "the restored database switched $segment, not on timeline $hex"
await_archived "$segment" >/dev/null
await_archived "$hex\\.history" >/dev/null

# Restored again, to the second instant, from the same base and archive.
throw_away
said=$(restore "$identity" "$second_instant") || fail "the restore to the second instant failed: $said"
start_restored
[[ $(opened) == "$stamp-a,$stamp-b" ]] || fail "restored to the second instant, the companies are: $(opened)"
second=$(timeline)
((second > first)) || fail "restored again, the database writes timeline $second, not beyond $first"
for _ in $(seq 1 60); do
  ready=$(sql "select count(*) from pg_ls_archive_statusdir() where name like '%.ready'")
  ((ready == 0)) && break
  sleep 1
done
((ready == 0)) ||
  fail "files are left to archive: $(sql "select string_agg(name, ' ') from pg_ls_archive_statusdir() where name like '%.ready'")"
[[ $(sql 'select failed_count from pg_stat_archiver') == 0 ]] ||
  fail "the archiver failed: $(sql 'select last_failed_wal from pg_stat_archiver')"

echo "point in time test passed: $base"
