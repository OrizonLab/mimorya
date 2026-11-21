#!/usr/bin/env bash
# Conservation of the backups and of the WAL archive (ADR-215, chantier 4,
# lot 70c), so that the database can be brought back to any instant of the
# last <days> days, and no further:
#   infra/backup/prune.sh <days> [--dry-run]
# The archive is pruned first, in the database's container, which must run
# (infra/postgres/prune-wal.sh): it keeps the base backups the period needs,
# on each timeline in it, and the WAL from the earliest of their starts on.
# Then, in the backups' folder (MIMORYA_BACKUPS_DIR, backups/ at the
# repository's root by default), of the files backup.sh named
# (mimorya-<UTC time>.dump.age, .base.tar.gz.age), whose age is the UTC time
# of their name, when they began:
#   - a base backup stays when made since the cutoff (now less <days> days),
#     or when it is the last made before a base the archive keeps ended:
#     its file;
#   - a dump stays when made since the cutoff, or when it is the last made
#     before: the oldest instant of the period lies after it;
#   - without an archive (archiving off, or no base's history kept in it), a
#     base backup is kept as a dump is.
# A file named otherwise, a symbolic link or a partial file is never
# touched. A base copied elsewhere and kept longer needs a copy of the
# archive kept as long. With --dry-run, what would be removed is said,
# nothing is. Run it each day (cron), after the base backup. The stack's
# variables and compose files as for backup.sh. Needs GNU date (Linux). Its
# messages are for the operator, in French (ADR-013).
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

running=$(docker compose ps --status running --services 2>/dev/null) || fail "impossible de lire l'état de la pile"
grep -qx db <<<"$running" || fail "la base de la pile doit tourner ; rien n'est élagué"
mode=$(docker compose exec -T db psql --username mimorya --dbname mimorya -tAc 'show archive_mode') ||
  fail "la base de la pile ne répond pas ; rien n'est élagué"

# The archive first: the bases it keeps tell which files to keep.
ends=
if [[ $mode == on ]]; then
  said=$(docker compose exec -T -u postgres db bash /usr/local/bin/prune-wal.sh "$days" ${dry:+"$dry"} 2>&1) ||
    fail "l'archive n'a été élaguée qu'en partie, ou pas du tout (sans trou : du plus ancien segment au plus récent) ; les sauvegardes n'ont pas été touchées : $said"
  grep -v '^kept-base-ends' <<<"$said" || true
  ends=$(sed -n 's/^kept-base-ends *//p' <<<"$said")
else
  echo "Archive des WAL désactivée : seules les sauvegardes sont élaguées"
fi

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

# The files of the backups on the standard input to remove: made before the
# cutoff, and neither the last made before one of the times $1 (the ends of
# the bases the archive keeps) nor, without them, the last made before the
# cutoff.
removed() {
  awk -v cutoff="$cutoff" -v ends="$1" '
    { stamp[NR] = $1; file[NR] = substr($0, length($1) + 2) }
    END {
      k = split(ends, e, " ")
      if (!k) { k = 1; e[1] = cutoff - 1 }
      for (j = 1; j <= k; j++) {
        best = 0
        for (i = 1; i <= NR; i++)
          if (stamp[i] <= e[j] + 0 && (!best || stamp[i] > stamp[best])) best = i
        if (best) keep[best] = 1
      }
      for (i = 1; i <= NR; i++)
        if (stamp[i] < cutoff + 0 && !(i in keep)) print file[i]
    }'
}

shopt -s nullglob
if [[ -d $folder ]]; then
  for kind in dump.age base.tar.gz.age; do
    backups=$(made "$kind")
    [[ -n $backups ]] || continue
    [[ $kind == base.tar.gz.age ]] && times=$ends || times=
    while IFS= read -r file; do
      [[ -n $file ]] || continue
      [[ -n $dry ]] || rm -f -- "$file" || fail "impossible de retirer $file"
      echo "Sauvegarde $verb : $file"
    done < <(removed "$times" <<<"$backups")
  done
else
  echo "Aucun dossier de sauvegardes : $folder"
fi
