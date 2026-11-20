#!/usr/bin/env bash
# Checks the conservation of a running production stack's backups and WAL
# archive, and the archive's state for monitoring (ADR-215):
#   - archive-status.sh says the archive up to date (status 0); with
#     --base-within, a base archived since; an archiving that fails (the
#     system's folder made read-only) said with its WAL (1), then up to date
#     once it archives again;
#   - prune.sh 30, after a --dry-run that removes nothing, on made-up
#     folders of the archive:
#       - the folder of another system whose files are all older, removed
#         whole;
#       - in a folder with bases, the files before the earliest start among
#         the bases kept (those archived since the cutoff, the last before)
#         removed, whatever their timeline; the timelines' histories and a
#         hidden partial file kept;
#       - a timeline begun by a restore inside the period, forked before
#         bases of its parent's abandoned branch: the parent's last base
#         before the fork kept, and what follows it, which the new
#         timeline's first instants need;
#       - a segment number that reads as a number in scientific notation
#         (…01E1) compared as hexadecimal; a folder without any base left
#         whole; in the current system's folder, every base and what it
#         needs kept;
#     and in the backups' folder: of the base backups, those made since the
#     cutoff and the last made before each base the archive keeps ended (one
#     begun before the cutoff and ended after it among them); of the dumps,
#     those made since and the last before; a file named otherwise, a
#     partial file, kept;
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
count_bases() { archived '[0-9A-F]{24}\.[0-9A-F]{8}\.backup' | wc -l; }
bases_before=$(count_bases)
MIMORYA_BACKUP_RECIPIENTS=$(age-keygen -y "$work/identity.txt") \
  infra/backup/backup.sh --base "$work/base.tar.gz.age" >/dev/null
# Its history is archived after the base ends.
for _ in $(seq 1 60); do
  (($(count_bases) > bases_before)) && break
  sleep 1
done
said=$(status_becomes 5 0 --base-within 1)
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

# Made-up files, as the database's, aged: folder, name, age in minutes.
sum=$(printf '%064d' 0 | tr 0 a)
day=1440
named() { date -u -d "$1 minutes ago" +%Y%m%dT%H%M%SZ; }
made_up() {
  local in=$archive/$1 name=$2 age=$3
  as_root install -d -m 700 -o "$owner" -g "$owner" "$in"
  as_root install -m 600 -o "$owner" -g "$owner" /dev/null "$in/$name"
  as_root touch -d "$age minutes ago" "$in/$name"
}
segment_of() { printf '%08X%016X' "$1" "$2"; }
segment() { made_up "$1" "$(segment_of "$2" "$3").$sum.gz.age" "$4"; }
base() { made_up "$1" "$(segment_of "$2" "$3").00000028.backup.$sum.gz.age" "$4"; }
gone=1000000000000000001
pruned=1000000000000000002
notation=1000000000000000003
baseless=1000000000000000004
forked=1000000000000000005
straddling=1000000000000000006
chained=1000000000000000007
overlapping=1000000000000000008
fresh=1000000000000000009
segment $gone 1 1 $((40 * day))
base $gone 1 1 $((40 * day))
made_up $gone "00000002.history.$sum.gz.age" $((40 * day))
for n in 1 2 3 4 5 6 7 8 9 10; do
  segment $pruned 1 "$n" $(((55 - n * 5) * day))
done
base $pruned 1 2 $((50 * day))
base $pruned 1 5 $((40 * day))
base $pruned 1 8 $((10 * day))
made_up $pruned "$(segment_of 1 3).partial.$sum.gz.age" $((45 * day))
segment $pruned 2 4 $((44 * day))
made_up $pruned "00000002.history.$sum.gz.age" $((45 * day))
made_up $pruned ".$(segment_of 1 9).partial-AbCdEf" $((45 * day))
segment $notation 1 255 $((45 * day))
base $notation 1 256 $((40 * day))
segment $notation 1 256 $((40 * day))
base $notation 1 481 $((10 * day))
segment $notation 1 481 $((10 * day))
segment $baseless 1 1 $((40 * day))
segment $baseless 1 2 $((1 * day))
# Timeline 1, its bases at segments 2, 5 and 8, all older than the cutoff;
# 20 days ago, a restore to an instant of segment 3 began timeline 2.
for n in 1 2 3 4 5 6 7 8 9; do
  segment $forked 1 "$n" $(((47 - n) * day))
done
base $forked 1 2 $((45 * day))
base $forked 1 5 $((40 * day))
base $forked 1 8 $((33 * day))
made_up $forked "00000002.history.$sum.gz.age" $((20 * day))
for n in 3 4 5 6; do
  segment $forked 2 "$n" $(((35 - n * 5) * day))
done
base $forked 2 5 $((10 * day))
# A base that ended a day before the cutoff, one begun before it and ended
# after it.
segment $straddling 1 15 $((32 * day))
segment $straddling 1 16 $((31 * day + 5))
base $straddling 1 16 $((31 * day))
segment $straddling 1 17 $((30 * day + 30))
base $straddling 1 18 $((30 * day - 20))
segment $straddling 1 18 $((30 * day - 25))
segment $straddling 1 19 $((1 * day))
# Two restores in a row: timeline 2 forked at segment 4, after timeline 1's
# base at 2, before its bases at 5 and 6; timeline 3 forked from timeline 2
# at segment 8, before timeline 2's base at 9, within the period. Timeline
# 3's first instants need timeline 1's base at 2, not those at 5 and 6,
# which began after timeline 2 forked: two of them, so that keeping only the
# last two bases before timeline 3's own fork would lose it.
for n in 1 2 3 4 5 6 7; do
  segment $chained 1 "$n" $(((46 - n) * day))
done
base $chained 1 2 $((44 * day))
base $chained 1 5 $((41 * day))
base $chained 1 6 $((40 * day))
made_up $chained "00000002.history.$sum.gz.age" $((38 * day))
for n in 4 5 6 7 8 9 10; do
  segment $chained 2 "$n" $(((42 - n) * day))
done
base $chained 2 9 $((31 * day))
made_up $chained "00000003.history.$sum.gz.age" $((25 * day))
for n in 8 9 10 11; do
  segment $chained 3 "$n" $(((65 - n * 5) * day))
done
base $chained 3 10 $((15 * day))
# A base of timeline 1 began at segment 10, before the fork of timeline 2
# at segment 12, and ended after it: the base before it is kept too.
for n in 1 2 3 4 5 6 7 8 9 10 11 12 13; do
  segment $overlapping 1 "$n" $(((47 - n) * day))
done
base $overlapping 1 2 $((45 * day))
base $overlapping 1 10 $((34 * day + 720))
made_up $overlapping "00000002.history.$sum.gz.age" $((25 * day))
for n in 12 13 14; do
  segment $overlapping 2 "$n" $(((85 - n * 5) * day))
done
base $overlapping 2 14 $((10 * day))
# A timeline that just began, none of its segments archived yet: its fork
# is unknown, nothing is removed, and every base's file is kept, its
# oldest one's too. Timeline 1 is active: without the timeline that just
# began, its files before its last base before the cutoff would go.
segment $fresh 1 1 $((48 * day))
segment $fresh 1 2 $((40 * day))
segment $fresh 1 3 $((1 * day))
base $fresh 1 1 $((48 * day))
base $fresh 1 3 $((30 * day + 60))
made_up $fresh "00000002.history.$sum.gz.age" 1
# What two of them keep: all but the files before the earliest base kept,
# timeline 1's segment 1.
all_but_first() {
  as_root find "$archive/$1" -mindepth 1 -printf '%f\n' | sed "s/\.$sum\.gz\.age$//" |
    grep -vx "$(segment_of 1 1)" | sort | tr '\n' ' '
}
chained_expected=$(all_but_first $chained)
overlapping_expected=$(all_but_first $overlapping)

backups=$work/backups
mkdir "$backups"
# Named once, when they began: the names' times, not the files', tell their
# ages. A base's file is kept when it is the last begun before a base the
# archive keeps ended: 48 days (fresh, only it), 45 and 44 (forked,
# chained, overlapping), 41 and 40 (chained, pruned, notation), 34 and a
# half (overlapping), 31 and 30 (straddling, fresh), 15, 10 and now. Those
# begun 50 and 47 days ago serve none.
kept_backups=("mimorya-$(named $((35 * day))).dump.age" "mimorya-$(named $((5 * day))).dump.age"
  "mimorya-$(named $((48 * day + 10))).base.tar.gz.age" "mimorya-$(named $((45 * day + 10))).base.tar.gz.age"
  "mimorya-$(named $((40 * day + 30))).base.tar.gz.age" "mimorya-$(named $((35 * day))).base.tar.gz.age"
  "mimorya-$(named $((31 * day + 10))).base.tar.gz.age" "mimorya-$(named $((30 * day + 10))).base.tar.gz.age"
  "mimorya-$(named $((10 * day + 10))).base.tar.gz.age"
  "mimorya-$(named $((40 * day))).dump.age.partial.AbCdEf" mimorya-custom.dump.age notes.txt)
for name in "${kept_backups[@]}" "mimorya-$(named $((40 * day))).dump.age" \
  "mimorya-$(named $((50 * day))).base.tar.gz.age" "mimorya-$(named $((47 * day))).base.tar.gz.age"; do
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
# Looked at as root: the archive is the database's (700), even to see a name.
! as_root test -e "$archive/$gone" || fail "the other system's old folder was kept: $(remaining $gone)"
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
grep -q "Archive du système $baseless : aucune sauvegarde de base n'y sert la période, rien n'est retiré" <<<"$said" ||
  fail "the folder without bases was not said: $said"
# Timeline 2's first instants need timeline 1's base at segment 2, before
# the fork: only segment 1 goes.
expected="00000002.history $(segment_of 1 2).00000028.backup $(segment_of 1 5).00000028.backup"
expected+=" $(segment_of 1 8).00000028.backup $(segment_of 2 5).00000028.backup"
for n in 2 3 4 5 6 7 8 9; do expected+=" $(segment_of 1 "$n")"; done
for n in 3 4 5 6; do expected+=" $(segment_of 2 "$n")"; done
expected=$(tr ' ' '\n' <<<"$expected" | sort | tr '\n' ' ')
[[ $(remaining $forked) == "$expected" ]] ||
  fail "a restored timeline's first instants lost their base: $(remaining $forked), not $expected"
expected=$(printf '%s\n' "$(segment_of 1 16)" "$(segment_of 1 16).00000028.backup" "$(segment_of 1 17)" \
  "$(segment_of 1 18).00000028.backup" "$(segment_of 1 18)" "$(segment_of 1 19)" | sort | tr '\n' ' ')
[[ $(remaining $straddling) == "$expected" ]] ||
  fail "unexpected files kept around the cutoff: $(remaining $straddling), not $expected"
[[ $(remaining $chained) == "$chained_expected" ]] ||
  fail "two restores in a row lost the base their first instants need: $(remaining $chained), not $chained_expected"
[[ $(remaining $overlapping) == "$overlapping_expected" ]] ||
  fail "a base overlapping a fork took the place of the one before: $(remaining $overlapping), not $overlapping_expected"
[[ $(remaining $fresh | wc -w) == 6 ]] || fail "a timeline that just began lost files: $(remaining $fresh)"
grep -q "Archive du système $fresh : une ligne temporelle vient de commencer" <<<"$said" ||
  fail "the timeline that just began was not said: $said"
lost=$(comm -23 <(echo "$needed_before") <(needed "$earliest"))
[[ -z $lost ]] || fail "files the current system's bases need were pruned: $lost"
kept=$(find "$backups" -mindepth 1 -printf '%f\n' | sort | tr '\n' ' ')
expected=$(printf '%s\n' "${kept_backups[@]}" | sort | tr '\n' ' ')
[[ $kept == "$expected" ]] || fail "unexpected backups kept: $kept, not $expected"
as_root rm -rf "${archive:?}/$pruned" "${archive:?}/$notation" "${archive:?}/$baseless" \
  "${archive:?}/$forked" "${archive:?}/$straddling" "${archive:?}/$chained" "${archive:?}/$overlapping" \
  "${archive:?}/$fresh"

# The current system's folder, its files all made 40 days older, in the
# order they were archived, then a segment archived now, as the database
# does all the time: kept, the last base before the cutoff (the one made
# above, the newest) and what follows it too; it archives on.
newest_base=$(as_root find "$folder" -maxdepth 1 -name '*.backup.*' -printf '%T@ %f\n' | sort -n | tail -n 1)
newest_base=${newest_base#* }
start=${newest_base:0:24}
as_root find "$folder" -maxdepth 1 -type f -printf '%T@ %p\n' | while read -r stamp file; do
  as_root touch -d "@$((${stamp%.*} - 40 * 86400))" "$file"
done
# The folder's files, their names short, with their ages in days.
shown() {
  as_root find "$folder" -maxdepth 1 -type f -printf '%T@ %f\n' | sort -n |
    awk -v now="$(date +%s)" '{ sub(/\.[0-9a-f]{64}\.gz\.age$/, "", $2); printf "%s(%dd) ", $2, (now - $1) / 86400 }'
}
sql "insert into retention_marks values (3)" >/dev/null
await_archived "$(sql "select pg_walfile_name(pg_switch_wal())")" >/dev/null
aged=$(shown)
mkdir "$work/none"
said=$(MIMORYA_BACKUPS_DIR=$work/none infra/backup/prune.sh 30) || fail "the pruning of an aged archive failed: $said"
as_root test -d "$folder" && [[ -n $(archived "$start") && -n $(as_root find "$folder" -name "$newest_base") ]] ||
  fail "the current system's last base ($newest_base) or its first segment was removed: $said; before: $aged; after: $(shown)"
older=$(as_root find "$folder" -maxdepth 1 -type f -printf '%f\n' | awk -v first="${start:8}" '
  /^\./ { next }
  substr($0, 9, 9) != ".history." && (substr($0, 9, 16) "") < (first "")')
[[ -z $older ]] || fail "files before the last base's start were kept: $older; after: $(shown)"
sql "insert into retention_marks values (2)" >/dev/null
await_archived "$(sql "select pg_walfile_name(pg_switch_wal())")" >/dev/null
said=$(status_becomes 5 1 --base-within 24)
grep -q 'aucune sauvegarde de base archivée depuis 24 h' <<<"$said" || fail "the old base was not said: $said"

echo "archive retention test passed"
