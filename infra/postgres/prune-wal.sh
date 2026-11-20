#!/usr/bin/env bash
# Conservation of the WAL archive (ADR-215, chantier 4, lot 70c), run in the
# database's container as postgres by infra/backup/prune.sh:
#   prune-wal.sh <days> [--dry-run]
# A restore to a point in time replays the archive on a base backup that
# ended before the instant, on the instant's timeline or one it forked from
# (ADR-214). The server cannot read the bases nor the timelines' histories
# (encrypted); it reads the names and archiving times of their files: a
# base's history (<segment>.<offset>.backup) names its timeline and starting
# segment, and was archived when it ended; a timeline's history
# (<timeline>.history) was archived when that timeline began, at a restore.
# In each database system's folder, the bases kept are:
#   - those archived since the cutoff (now less <days> days);
#   - for each timeline with a file archived since the cutoff: its own last
#     base archived before the cutoff; without one (the timeline began
#     after it, or has no base yet), a restore along it needs, for its first
#     instants, the last base of its ancestry before its fork: on the parent
#     before the fork, or on the grandparent before the parent's fork, and
#     so on. The histories that would name them are encrypted: for each
#     lower timeline a, and each timeline u above a and up to it, the last
#     two bases of a that began before u's first segment are kept, the
#     second for a last one that began before a fork and ended after it.
#     Timelines it does not descend from add bases kept for nothing.
# A timeline that just began (its history archived since the cutoff, none
# of its segments yet) holds back its folder: nothing is removed until its
# first segment tells where it forked.
# The segments, partial segments and bases' histories before the earliest
# start among the bases kept go, whatever their timeline. Timelines'
# histories are always kept: tiny, a restore reads them to choose its
# timeline. A folder whose bases serve nothing of the period keeps
# everything. The folder of another database system (one a dump was
# restored over, ADR-166) whose last file is older than the cutoff is
# removed whole: nothing of the period is in it. The current system's folder
# never is. Files go oldest segment first, so that what is left of a
# timeline never has a hole. With --dry-run, what would be removed is said,
# nothing is. A file's age is when it was archived: a copy keeps it with
# `cp -p` or `rsync -a`. Last, a line `kept-base-ends <times>` gives when
# each base kept ended (seconds since 1970), for prune.sh to keep their
# files. Its other messages are for the operator, in French (ADR-013).
set -euo pipefail
export LC_ALL=C

root=/var/lib/postgresql/wal-archive
fail() {
  echo "Conservation de l'archive : $1" >&2
  exit 1
}

days=${1:-}
dry=${2:-}
[[ $days =~ ^[1-9][0-9]{0,3}$ ]] || fail "un nombre de jours, de 1 à 9999"
[[ -z $dry || $dry == --dry-run ]] || fail "option inconnue : $dry (seule --dry-run est reconnue)"
current=$(pg_controldata /var/lib/postgresql/data | sed -n 's/^Database system identifier: *//p') || true
[[ $current =~ ^[0-9]+$ ]] || fail "identifiant du système de la base illisible"
[[ -d $root && -r $root && -w $root && -x $root ]] || fail "le dossier de l'archive n'est pas à la base"
cutoff=$(($(date +%s) - days * 86400))
[[ -n $dry ]] && verb="serait retiré" || verb="retiré"

# Removes $1, or says it would.
remove() {
  [[ -n $dry ]] || rm -rf -- "$1" || fail "impossible de retirer $1"
}

# The archived files of the folder $1, one per line, as
# <mtime> <kind> <timeline> <segment number> <file>: kind segment, partial,
# backup or history; the segment number, 16 hex digits, the same on every
# timeline (- for a timeline's history).
listed() {
  find "$1" -maxdepth 1 -type f -printf '%T@ %f\n' | while read -r stamp file; do
    if [[ $file =~ ^([0-9A-F]{8})([0-9A-F]{16})(\.partial|\.[0-9A-F]{8}\.backup)?\.[0-9a-f]{64}\.gz\.age$ ]]; then
      case ${BASH_REMATCH[3]} in
        '') kind=segment ;;
        .partial) kind=partial ;;
        *) kind=backup ;;
      esac
      echo "${stamp%.*} $kind ${BASH_REMATCH[1]} ${BASH_REMATCH[2]} $file"
    elif [[ $file =~ ^([0-9A-F]{8})\.history\.[0-9a-f]{64}\.gz\.age$ ]]; then
      echo "${stamp%.*} history ${BASH_REMATCH[1]} - $file"
    fi
  done
}

# The bases kept, as <start segment number> <end time>, one per line, from
# the files listed on the standard input. Timelines and segment numbers are
# compared as strings of one width, never as numbers (awk would read
# 00000000000001E1 as 10).
kept_bases() {
  awk -v cutoff="$cutoff" '
    $2 == "backup" { n++; ends[n] = $1; tl[n] = $3 ""; start[n] = $4 "" }
    $2 == "segment" && (!(($3 "") in low) || ($4 "") < low[$3 ""]) { low[$3 ""] = $4 "" }
    $1 >= cutoff { active[$3 ""] = 1 }
    END {
      for (i = 1; i <= n; i++) {
        if (ends[i] >= cutoff) kept[i] = 1
        lines[tl[i]] = 1
      }
      for (t in active) {
        if (!(t in low)) { print "fresh"; exit }
        best = 0
        for (i = 1; i <= n; i++)
          if (tl[i] == t && ends[i] < cutoff && (!best || ends[i] > ends[best])) best = i
        if (best) { kept[best] = 1; continue }
        for (a in lines) {
          if (!(a < t)) continue
          for (u in low) {
            if (!(a < u) || u > t) continue
            last = 0
            before = 0
            for (i = 1; i <= n; i++) {
              if (tl[i] != a || !(start[i] < low[u])) continue
              if (!last || ends[i] > ends[last]) { before = last; last = i }
              else if (!before || ends[i] > ends[before]) before = i
            }
            if (last) kept[last] = 1
            if (before) kept[before] = 1
          }
        }
      }
      for (i in kept) print start[i], ends[i]
    }'
}

ends=()
shopt -s nullglob
for folder in "$root"/*/; do
  folder=${folder%/}
  system=${folder##*/}
  [[ $system =~ ^[0-9]+$ ]] || continue
  files=$(listed "$folder")
  [[ -n $files ]] || continue
  newest=$(sort -n <<<"$files" | tail -n 1 | cut -d' ' -f1)
  if [[ $system != "$current" ]] && ((newest < cutoff)); then
    remove "$folder"
    echo "Archive du système $system $verb entière : son dernier fichier date de plus de $days jours"
    continue
  fi
  kept=$(kept_bases <<<"$files")
  if [[ $kept == fresh ]]; then
    echo "Archive du système $system : une ligne temporelle vient de commencer, sans segment encore archivé ; rien n'est retiré"
    continue
  fi
  if [[ -z $kept ]]; then
    echo "Archive du système $system : aucune sauvegarde de base n'y sert la période, rien n'est retiré"
    continue
  fi
  read -ra system_ends <<<"$(cut -d' ' -f2 <<<"$kept" | tr '\n' ' ')"
  ends+=("${system_ends[@]}")
  first=$(cut -d' ' -f1 <<<"$kept" | sort | head -n 1)
  # What no base kept needs, oldest segment first; never a timeline's history.
  old=$(awk -v first="$first" '$2 != "history" && ($4 "") < (first "")' <<<"$files" | sort -k4,4 -k5,5)
  count=0
  while read -r _ _ _ _ file; do
    [[ -n $file ]] || continue
    remove "$folder/$file"
    count=$((count + 1))
  done <<<"$old"
  echo "Archive du système $system : $count fichier(s) $verb, antérieurs au segment $first de la plus ancienne sauvegarde de base gardée"
done
echo "kept-base-ends ${ends[*]}"
