#!/usr/bin/env bash
# Checks the conservation of a running production stack's backups and WAL
# archive, and the archive's state for monitoring (ADR-215):
#   - archive-status.sh says the archive up to date (status 0); with
#     --base-within, a base archived since; an archiving that fails (the
#     system's folder made read-only) said with its WAL (1), then up to date
#     once it archives again;
#   - prune.sh 30, after a --dry-run that removes nothing:
#       - in the backups' folder, the dumps and bases made in the last 30
#         days kept, and of each kind the last one before; the older ones
#         removed; a file named otherwise, a partial file, kept;
#       - in the archive, the folder of another system whose files are all
#         older removed whole; in a folder with bases, the files before the
#         earliest start among the bases kept removed, whatever their
#         timeline, the timelines' histories and a hidden partial file
#         kept; a segment number that reads as a number in scientific
#         notation (…01E1) compared as hexadecimal; a folder without any
#         base left whole; in the current system's folder, every base and
#         what it needs kept;
#   - the current system's folder, its files all made 40 days older, is
#     never removed: the last base before the cutoff and what follows it
#     are kept, nothing before it but the timelines' histories, and it
#     archives on; --base-within 24 then says no base since.
# It adds made-up folders and files to the archive, and ages the current
# system's: for a stack thrown away afterwards (the CI's), hence
# DISPOSABLE_STACK=yes, as the last of its tests. The stack's variables and
# compose files as for wal-archive-test.sh; the archive read and written as
# root (archive-checks.sh). Needs age-keygen and GNU date.
#   DISPOSABLE_STACK=yes infra/docker/archive-retention-test.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh
source infra/docker/archive-checks.sh

check_name="archive retention test"
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it ages the stack's archive: set DISPOSABLE_STACK=yes on a throwaway stack only"
command -v age-keygen >/dev/null || fail "age is not installed"
archive=${MIMORYA_WAL_ARCHIVE:-./wal-archive}
[[ $archive == /* ]] || archive=$PWD/${archive#./}

sql() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc "$1" || fail "cannot run: $1"
}
# Waits a while ($1 seconds) for archive-status.sh to exit with $2; prints
# what it said.
status_becomes() {
  local said code
  for _ in $(seq 1 "$1"); do
    code=0
    said=$(infra/backup/archive-status.sh "${@:3}" 2>&1) || code=$?
    if ((code == $2)); then
      echo "$said"
      return
    fi
    sleep 1
  done
  fail "archive-status.sh did not exit with $2 within $1 s: $said"
}

work=$(mktemp -d)
trap 'as_root rm -rf "$work"' EXIT
system=$(sql "select system_identifier from pg_control_system()")
folder=$archive/$system
owner=$(as_root stat -c %u "$folder") || fail "the stack archives nothing in $folder"

# The archive's state.
said=$(status_becomes 5 0)
[[ $said == "Archive à jour : "* ]] || fail "unexpected state: $said"
age-keygen -o "$work/identity.txt" 2>/dev/null
MIMORYA_BACKUP_RECIPIENTS=$(age-keygen -y "$work/identity.txt") \
  infra/backup/backup.sh --base "$work/base.tar.gz.age" >/dev/null
said=$(status_becomes 60 0 --base-within 1)
grep -q 'dernière sauvegarde de base archivée il y a 0 min' <<<"$said" || fail "no recent base said: $said"
as_root chmod 500 "$folder"
sql "create table if not exists retention_marks (x int); insert into retention_marks values (1)" >/dev/null
segment=$(sql "select pg_walfile_name(pg_switch_wal())")
said=$(status_becomes 30 1)
grep -qF "Archive à vérifier : le dernier archivage a échoué (WAL $segment" <<<"$said" ||
  fail "the failed archiving was not said: $said"
as_root chmod 700 "$folder"
# A segment switched wakes the archiver, which tries again at once.
sql "insert into retention_marks values (2)" >/dev/null
sql "select pg_switch_wal()" >/dev/null
await_archived "$segment" >/dev/null
said=$(status_becomes 60 0)

# Made-up files, as the database's, aged: name, then age in days.
sum=$(printf '%064d' 0 | tr 0 a)
day() { date -u -d "$1 days ago" +%Y%m%dT%H%M%SZ; }
made_up() {
  local in=$archive/$1 name=$2 age=$3
  as_root install -d -m 700 -o "$owner" -g "$owner" "$in"
  as_root install -m 600 -o "$owner" -g "$owner" /dev/null "$in/$name"
  as_root touch -d "$age days ago" "$in/$name"
}
segment_of() { printf '%08X%016X' "$1" "$2"; }
gone=1000000000000000001
pruned=1000000000000000002
notation=1000000000000000003
baseless=1000000000000000004
made_up $gone "$(segment_of 1 1).$sum.gz.age" 40
made_up $gone "$(segment_of 1 1).00000028.backup.$sum.gz.age" 40
made_up $gone "00000002.history.$sum.gz.age" 40
for n in 1 2 3 4 5 6 7 8 9 10; do
  made_up $pruned "$(segment_of 1 "$n").$sum.gz.age" $((55 - n * 5))
done
made_up $pruned "$(segment_of 1 2).00000028.backup.$sum.gz.age" 50
made_up $pruned "$(segment_of 1 5).00000028.backup.$sum.gz.age" 40
made_up $pruned "$(segment_of 1 8).00000028.backup.$sum.gz.age" 10
made_up $pruned "$(segment_of 1 3).partial.$sum.gz.age" 45
made_up $pruned "$(segment_of 2 4).$sum.gz.age" 44
made_up $pruned "00000002.history.$sum.gz.age" 45
made_up $pruned ".$(segment_of 1 9).partial-AbCdEf" 45
made_up $notation "$(segment_of 1 255).$sum.gz.age" 45
made_up $notation "$(segment_of 1 256).00000028.backup.$sum.gz.age" 40
made_up $notation "$(segment_of 1 256).$sum.gz.age" 40
made_up $notation "$(segment_of 1 481).00000028.backup.$sum.gz.age" 10
made_up $notation "$(segment_of 1 481).$sum.gz.age" 10
made_up $baseless "$(segment_of 1 1).$sum.gz.age" 40
made_up $baseless "$(segment_of 1 2).$sum.gz.age" 1

backups=$work/backups
mkdir "$backups"
# Named once: the names' times, not the files', tell their ages.
kept_backups=("mimorya-$(day 35).dump.age" "mimorya-$(day 5).dump.age"
  "mimorya-$(day 40).base.tar.gz.age" "mimorya-$(day 10).base.tar.gz.age"
  "mimorya-$(day 40).dump.age.partial.AbCdEf" mimorya-custom.dump.age notes.txt)
for name in "${kept_backups[@]}" "mimorya-$(day 40).dump.age" "mimorya-$(day 50).base.tar.gz.age"; do
  touch "$backups/$name"
done
listing() {
  as_root find "$archive" "$backups" -mindepth 1 -printf '%P\n' | sort
}
# The current system's files its bases need: from the earliest start on,
# and the timelines' histories.
needed() {
  as_root find "$folder" -maxdepth 1 -type f -printf '%f\n' | awk -v first="$1" '
    /^\./ { next }
    substr($0, 9, 9) == ".history." || (substr($0, 9, 16) "") >= (first "")' | sort
}
earliest=$(archived '[0-9A-F]{24}\.[0-9A-F]{8}\.backup' | sed 's|.*/||' | cut -c9-24 | sort | head -n 1)
[[ -n $earliest ]] || fail "the current system has no base archived"
needed_before=$(needed "$earliest")

# A dry run removes nothing, and says what it would.
before=$(listing)
said=$(MIMORYA_BACKUPS_DIR=$backups infra/backup/prune.sh 30 --dry-run) || fail "the dry run failed: $said"
[[ $(listing) == "$before" ]] || fail "the dry run removed files"
grep -q "Archive du système $gone serait retiré entière" <<<"$said" &&
  grep -q "Sauvegarde serait retirée : $backups/mimorya-.*\\.dump\\.age" <<<"$said" ||
  fail "the dry run did not say what it would remove: $said"

said=$(MIMORYA_BACKUPS_DIR=$backups infra/backup/prune.sh 30) || fail "the pruning failed: $said"
remaining() { as_root find "$archive/$1" -mindepth 1 -printf '%f\n' | sed "s/\\.$sum\\.gz\\.age$//" | sort | tr '\n' ' '; }
[[ ! -e $archive/$gone ]] || fail "the other system's old folder was kept: $(remaining $gone)"
expected=".$(segment_of 1 9).partial-AbCdEf 00000002.history"
for n in 5 6 7 8 9 10; do expected+=" $(segment_of 1 "$n")"; done
expected+=" $(segment_of 1 5).00000028.backup $(segment_of 1 8).00000028.backup"
expected=$(tr ' ' '\n' <<<"$expected" | sort | tr '\n' ' ')
[[ $(remaining $pruned) == "$expected" ]] ||
  fail "unexpected files kept with bases: $(remaining $pruned), not $expected"
expected=$(printf '%s\n' "$(segment_of 1 256)" "$(segment_of 1 256).00000028.backup" "$(segment_of 1 481)" \
  "$(segment_of 1 481).00000028.backup" | sort | tr '\n' ' ')
[[ $(remaining $notation) == "$expected" ]] ||
  fail "segment numbers were not compared as hexadecimal: $(remaining $notation), not $expected"
[[ $(remaining $baseless | wc -w) == 2 ]] || fail "a folder without bases lost files: $(remaining $baseless)"
grep -q "Archive du système $baseless : aucune sauvegarde de base archivée, rien n'est retiré" <<<"$said" ||
  fail "the folder without bases was not said: $said"
lost=$(comm -23 <(echo "$needed_before") <(needed "$earliest"))
[[ -z $lost ]] || fail "files the current system's bases need were pruned: $lost"
kept=$(find "$backups" -mindepth 1 -printf '%f\n' | sort | tr '\n' ' ')
expected=$(printf '%s\n' "${kept_backups[@]}" | sort | tr '\n' ' ')
[[ $kept == "$expected" ]] || fail "unexpected backups kept: $kept, not $expected"
as_root rm -rf "${archive:?}/$pruned" "${archive:?}/$notation" "${archive:?}/$baseless"

# The current system's folder, its files all made 40 days older, in the
# order they were archived: kept, the last base before the cutoff (the one
# made above, the newest) and what follows it too; it archives on.
newest_base=$(as_root find "$folder" -maxdepth 1 -name '*.backup.*' -printf '%T@ %f\n' | sort -n | tail -n 1)
newest_base=${newest_base#* }
start=${newest_base:0:24}
as_root find "$folder" -maxdepth 1 -type f -printf '%T@ %p\n' | while read -r stamp file; do
  as_root touch -d "@$((${stamp%.*} - 40 * 86400))" "$file"
done
said=$(infra/backup/prune.sh 30) || fail "the pruning of an aged archive failed: $said"
[[ -d $folder && -n $(archived "$start") && -n $(as_root find "$folder" -name "$newest_base") ]] ||
  fail "the current system's last base or its first segment was removed"
older=$(as_root find "$folder" -maxdepth 1 -type f -printf '%f\n' | awk -v first="${start:8}" '
  /^\./ { next }
  substr($0, 9, 9) != ".history." && (substr($0, 9, 16) "") < (first "")')
[[ -z $older ]] || fail "files before the last base's start were kept: $older"
sql "insert into retention_marks values (2)" >/dev/null
await_archived "$(sql "select pg_walfile_name(pg_switch_wal())")" >/dev/null
said=$(status_becomes 5 1 --base-within 24)
grep -q 'aucune sauvegarde de base archivée depuis 24 h' <<<"$said" || fail "the old base was not said: $said"

echo "archive retention test passed"
