#!/usr/bin/env bash
# Restores a running production stack to a point in time (ADR-214), as an
# operator would after a mistake: a base backup (backup.sh --base), then the
# archived WAL (ADR-206) replayed up to a chosen instant, into the stack's
# database thrown away. An instant is noted before the base; then three
# companies are opened one after the other, an instant noted between each,
# and the segment that holds them archived, then another.
#   - The restore is refused while the stack's database runs, while it holds
#     files, for an instant that could end its setting or without its zone,
#     with an identity that does not open the base, when the archive's
#     folder holds none of this database's, and for a timeline named
#     `latest`.
#   - What would end the recovery short stops it, said with the reason: a
#     segment whose contents differ from its name, a segment missing while
#     a later one is archived (without an instant, all the archive
#     replayed), an archive's folder that is not the database's (a copy made
#     as root).
#   - An instant before the base began is refused before any recovery: no
#     new timeline is taken.
#   - Restored to the first instant, the stack serves the first company,
#     whose administrator signs in, and neither of the others. Its database
#     is the same system, on the next timeline, said in decimal, without the
#     recovery's settings nor its container left. A fourth company opened
#     there is archived on the new timeline, beside the former one, with
#     its history.
#   - Restored again from the same base and archive to the second instant,
#     on the base's own timeline: the first two companies, not the others,
#     on a timeline beyond both former ones.
#   - Restored a third time, all the archive replayed along the first
#     restore's timeline, named by its number: the first and the fourth
#     companies, on a timeline beyond the others. The archiver has nothing
#     left to archive and has failed none, the three restores' files side by
#     side.
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
start_stack() {
  docker compose up -d >/dev/null 2>&1 || fail "the stack did not start"
  wait_for_stack "$base"
}
# A copy of the archive's folder of this system, made as `cp` $1 makes it,
# as the database's (-a) or as root's (-r); prints the copy's root.
copied_archive() {
  local copy
  copy=$(mktemp -d -p "$work")
  as_root chmod 700 "$copy"
  as_root cp "$1" "$folder" "$copy/"
  echo "$copy"
}

work=$(mktemp -d)
trap 'as_root rm -rf "$work"' EXIT
age-keygen -o "$work/other.txt" 2>/dev/null

wait_for_stack "$base"
system=$(sql "select system_identifier from pg_control_system()")
folder=$archive/$system
owner=$(as_root stat -c %u "$folder") || fail "the stack archives nothing in $folder"
before=$(timeline)
too_early=$(noted_instant)
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
await_archived "$(switched)" >/dev/null

# The refusals, each before anything is changed or with the database left
# to throw away, as the script says.
refused 'la base de la pile est démarrée' "$identity" "$first_instant"
refused 'instant illisible' "$identity" "$first_instant'; select 1"
refused 'instant illisible' "$identity" "${first_instant%+*}"
MIMORYA_RESTORE_TIMELINE=latest refused 'MIMORYA_RESTORE_TIMELINE : current ou le numéro' \
  "$identity" "$first_instant"
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
damaged=$(copied_archive -a)
head -c 16777216 /dev/zero | gzip | age --recipient "$recipient" >"$work/zeros.gz.age"
as_root install -m 600 -o "$owner" -g "$owner" "$work/zeros.gz.age" "$damaged/$system/${file##*/}"
MIMORYA_WAL_ARCHIVE=$damaged refused "la récupération s'est arrêtée" "$identity" "$second_instant"
grep -qF "Restauration arrêtée au WAL $start : son contenu ne correspond pas à son nom" <<<"$refusal" ||
  fail "the damaged segment was not said: $refusal"
throw_away

# A copy without the segment holding the companies, a later one archived:
# replaying all the archive stops at the hole.
holed=$(copied_archive -a)
as_root find "$holed/$system" -maxdepth 1 -regextype posix-extended \
  -regex ".*/$segment\.[0-9a-f]{64}\.gz\.age" -delete
MIMORYA_WAL_ARCHIVE=$holed refused "la récupération s'est arrêtée" "$identity"
grep -qF "Restauration arrêtée au WAL $segment : absent de l'archive, qui tient pourtant" <<<"$refusal" ||
  fail "the hole was not said: $refusal"
throw_away

# A copy made as root: its files would seem absent to the database.
rooted=$(copied_archive -r)
MIMORYA_WAL_ARCHIVE=$rooted refused "la récupération s'est arrêtée" "$identity"
grep -qF "n'est pas à la base (uid 999, en 700)" <<<"$refusal" ||
  fail "the archive's folder not the database's was not said: $refusal"
throw_away

# An instant before the base began: refused before any recovery.
refused "ne vient pas au moins une seconde après le début de la sauvegarde de base" \
  "$identity" "$too_early"
throw_away

# Restored to the first instant.
said=$(restore "$identity" "$first_instant") || fail "the restore to the first instant failed: $said"
grep -qF "Base restaurée jusqu'à $first_instant, ligne temporelle" <<<"$said" ||
  fail "unexpected restore: $said"
start_stack
check_headers_and_health "$base"
[[ $(opened) == "$stamp-a" ]] || fail "restored to the first instant, the companies are: $(opened)"
session=$(sign_in "$base" "admin@$stamp-a.example" "$password")
check_company_of "$base" "$session" 'Entreprise A'
[[ $(sql "select system_identifier from pg_control_system()") == "$system" ]] ||
  fail "the restored database is another system"
first=$(timeline)
# The refused instant began no timeline: the first restore takes the next.
((first == before + 1)) || fail "the restored database writes timeline $first, not $((before + 1))"
grep -qF "ligne temporelle $first ;" <<<"$said" || fail "the timeline was not said in decimal: $said"
[[ -z $(sql 'show restore_command') && -z $(sql 'show recovery_target_time') ]] ||
  fail "the recovery's settings were kept"
[[ -z $(docker ps -aq --filter name=mimorya-restore-) ]] || fail "the recovery's container was left"
open_company "$base" "$stamp-d" 'Entreprise D' "$password" >/dev/null
hex=$(printf '%08X' "$first")
segment=$(switched)
[[ $segment == "$hex"* ]] || fail "the restored database switched $segment, not on timeline $hex"
await_archived "$segment" >/dev/null
await_archived "$hex\\.history" >/dev/null

# Restored again, to the second instant, from the same base and archive.
throw_away
said=$(restore "$identity" "$second_instant") || fail "the restore to the second instant failed: $said"
start_stack
[[ $(opened) == "$stamp-a,$stamp-b" ]] || fail "restored to the second instant, the companies are: $(opened)"
second=$(timeline)
((second > first)) || fail "restored again, the database writes timeline $second, not beyond $first"

# Restored a third time, all the archive replayed along the first restore's
# timeline: the base's own up to its start, then its own.
throw_away
said=$(MIMORYA_RESTORE_TIMELINE=$first restore "$identity") ||
  fail "the restore along timeline $first failed: $said"
start_stack
[[ $(opened) == "$stamp-a,$stamp-d" ]] ||
  fail "restored along timeline $first, the companies are: $(opened)"
third=$(timeline)
((third > second)) || fail "restored a third time, the database writes timeline $third, not beyond $second"
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
