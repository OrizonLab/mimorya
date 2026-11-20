#!/usr/bin/env bash
# Conservation of the WAL archive (ADR-215, chantier 4, lot 70c), run in the
# database's container as postgres by infra/backup/prune.sh:
#   prune-wal.sh <days> [--dry-run]
# A restore to a point in time replays the archive on a base backup begun
# before the instant (ADR-214). To reach any instant of the last <days>
# days, each database system's folder keeps the base backups archived (their
# histories, <segment>.<offset>.backup) since then, and the last one before;
# of the WAL, what these bases need: the segments, partial segments and
# bases' histories from the earliest start among them on, whatever their
# timeline. Timelines' histories are always kept: they are tiny, and a
# restore reads them to choose its timeline. A folder without any base's
# history keeps everything. The folder of another database system (one a
# dump was restored over, ADR-166) whose last file is older than <days> days
# is removed whole: nothing of the window is in it. The current system's
# folder never is. Files are removed oldest segment first, so that what is
# left of a timeline never has a hole. With --dry-run, what would be removed
# is said, nothing is. A file's age is when it was archived: a copy of the
# archive keeps it with `cp -p` or `rsync -a`. Its messages are for the
# operator, in French (ADR-013).
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
now=$(date +%s)
cutoff=$((now - days * 86400))
[[ -n $dry ]] && verb="serait retiré" || verb="retiré"

# Removes $1, or says it would.
remove() {
  [[ -n $dry ]] || rm -rf -- "$1" || fail "impossible de retirer $1"
}

# The archived files of the folder $1, one per line, as
# <mtime> <kind> <segment number> <file>: kind segment, partial, backup or
# history; the segment number, 16 hex digits, the same on every timeline.
listed() {
  find "$1" -maxdepth 1 -type f -printf '%T@ %f\n' | while read -r stamp file; do
    if [[ $file =~ ^[0-9A-F]{8}([0-9A-F]{16})(\.partial|\.[0-9A-F]{8}\.backup)?\.[0-9a-f]{64}\.gz\.age$ ]]; then
      case ${BASH_REMATCH[2]} in
        '') kind=segment ;;
        .partial) kind=partial ;;
        *) kind=backup ;;
      esac
      echo "${stamp%.*} $kind ${BASH_REMATCH[1]} $file"
    elif [[ $file =~ ^[0-9A-F]{8}\.history\.[0-9a-f]{64}\.gz\.age$ ]]; then
      echo "${stamp%.*} history - $file"
    fi
  done
}

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
  # The bases kept: those archived since the cutoff, and the last before.
  bases=$(awk '$2 == "backup"' <<<"$files" | sort -n)
  if [[ -z $bases ]]; then
    echo "Archive du système $system : aucune sauvegarde de base archivée, rien n'est retiré"
    continue
  fi
  kept=$(awk -v cutoff="$cutoff" '$1 >= cutoff' <<<"$bases")
  before=$(awk -v cutoff="$cutoff" '$1 < cutoff' <<<"$bases" | tail -n 1)
  first=$(printf '%s\n%s\n' "$kept" "$before" | awk 'NF { print $3 }' | sort | head -n 1)
  # What no base kept needs, oldest segment first; never a timeline's
  # history. Segment numbers compared as strings, of one width: as numbers,
  # awk would read 00000000000001E1 as 10.
  old=$(awk -v first="$first" '$2 != "history" && ($3 "") < (first "")' <<<"$files" | sort -k3,3 -k4,4)
  count=0
  while read -r _ _ _ file; do
    [[ -n $file ]] || continue
    remove "$folder/$file"
    count=$((count + 1))
  done <<<"$old"
  echo "Archive du système $system : $count fichier(s) $verb, antérieurs au segment $first de la plus ancienne sauvegarde de base gardée"
done
