#!/usr/bin/env bash
# Conservation of the backups and of the WAL archive (ADR-215, chantier 4,
# lot 70c), so that the database can be brought back to any instant of the
# last <days> days, and no further:
#   infra/backup/prune.sh <days> [--dry-run]
# In the backups' folder (MIMORYA_BACKUPS_DIR, backups/ at the repository's
# root by default), the dumps and base backups backup.sh named
# (mimorya-<UTC time>.dump.age, .base.tar.gz.age) are kept when made in the
# last <days> days, and of each kind the last one made before; the others
# are removed. A file named otherwise is never touched, nor a partial file.
# Then the archive is pruned to what the base backups kept need, in the
# database's container (infra/postgres/prune-wal.sh), which must run. A base
# copied elsewhere and kept longer needs the archive longer too: run this
# with as many days. With --dry-run, what would be removed is said, nothing
# is. Run it each day (cron), after the base backup. The stack's variables
# and compose files as for backup.sh. Needs GNU date (Linux). Its messages
# are for the operator, in French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la conservation : $1" >&2
  exit 1
}

days=${1:-}
dry=${2:-}
(($# == 1 || $# == 2)) || fail "usage : infra/backup/prune.sh <jours> [--dry-run]"
[[ $days =~ ^[1-9][0-9]{0,3}$ ]] || fail "un nombre de jours, de 1 à 9999"
[[ -z $dry || $dry == --dry-run ]] || fail "option inconnue : $dry (seule --dry-run est reconnue)"
folder=${MIMORYA_BACKUPS_DIR:-}
[[ -z $folder || $folder == /* ]] || folder=$PWD/$folder
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
folder=${folder:-$PWD/backups}
cutoff=$(($(date +%s) - days * 86400))
[[ -n $dry ]] && verb="serait retirée" || verb="retirée"

# The backups of the kind $1 (dump.age, base.tar.gz.age), one per line, as
# <seconds since 1970> <file>, oldest first, by the UTC time of their name.
made() {
  local file name stamp
  for file in "$folder"/mimorya-*."$1"; do
    name=${file##*/}
    [[ $name =~ ^mimorya-([0-9]{4})([0-9]{2})([0-9]{2})T([0-9]{2})([0-9]{2})([0-9]{2})Z\.(dump\.age|base\.tar\.gz\.age)$ &&
      ${BASH_REMATCH[7]} == "$1" && -f $file && ! -L $file ]] || continue
    stamp=$(date -u -d "${BASH_REMATCH[1]}-${BASH_REMATCH[2]}-${BASH_REMATCH[3]} ${BASH_REMATCH[4]}:${BASH_REMATCH[5]}:${BASH_REMATCH[6]}" +%s) ||
      continue
    echo "$stamp $file"
  done | sort -n
}

shopt -s nullglob
if [[ -d $folder ]]; then
  for kind in dump.age base.tar.gz.age; do
    backups=$(made "$kind")
    [[ -n $backups ]] || continue
    # The last one made before the cutoff stays: the oldest instant of the
    # window lies after it.
    last_before=$(awk -v cutoff="$cutoff" '$1 < cutoff' <<<"$backups" | tail -n 1)
    while read -r stamp file; do
      ((stamp < cutoff)) && [[ "$stamp $file" != "$last_before" ]] || continue
      [[ -n $dry ]] || rm -f -- "$file" || fail "impossible de retirer $file"
      echo "Sauvegarde $verb : $file"
    done <<<"$backups"
  done
else
  echo "Aucun dossier de sauvegardes : $folder"
fi

running=$(docker compose ps --status running --services 2>/dev/null) || fail "impossible de lire l'état de la pile"
grep -qx db <<<"$running" || fail "la base de la pile doit tourner pour élaguer son archive"
docker compose exec -T -u postgres db bash /usr/local/bin/prune-wal.sh "$days" ${dry:+"$dry"} ||
  fail "l'archive n'a pas été élaguée"
