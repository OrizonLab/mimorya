#!/usr/bin/env bash
# PostgreSQL's restore command in a restore to a point in time (ADR-214,
# chantier 4, lot 70b), run by the startup process as postgres from the data
# directory: restore-wal.sh %f %p. The file PostgreSQL asks for (a WAL
# segment, a timeline's history) is looked for in the archive of the
# database system being restored, as archive-wal.sh wrote it:
#   /var/lib/postgresql/wal-archive/<system identifier>/<name>.<sha256>.gz.age
# It is decrypted with the identity given for the restore alone (written by
# restore-base.sh in the container's memory, never on a volume),
# decompressed, and checked against the SHA-256 its name holds before it is
# handed over at %p. Absent, the command fails quietly: PostgreSQL asks for
# files past the end of the archive, and for timelines' histories that do not
# exist, to choose a new one. Whatever would end the restore short without a
# word stops it instead (status 126, which PostgreSQL takes as fatal): a
# file present but unreadable (another identity, a damaged file, a sum that
# differs) or archived twice, a segment absent while a later one of its
# timeline is archived (a hole), an archive's folder the database cannot
# read, whose files would seem absent. Its messages go to PostgreSQL's log,
# for the operator, in French (ADR-013).
set -euo pipefail
# Segments' names compared as bytes, never by a locale's collation.
export LC_ALL=C

name=${1:-}
target=${2:-}
root=/var/lib/postgresql/wal-archive
identity=/dev/shm/mimorya-restore/identity

stop() {
  echo "Restauration arrêtée au WAL ${name:-?} : $1" >&2
  exit 126
}

# What a recovery asks for: segments and timelines' histories.
[[ $name =~ ^[0-9A-F]{24}$|^[0-9A-F]{8}\.history$ ]] || stop "nom inattendu"
[[ -n $target ]] || stop "aucune destination"
[[ -r $identity ]] || stop "aucune identité pour la restauration"

system=$(pg_controldata . | sed -n 's/^Database system identifier: *//p') || true
[[ $system =~ ^[0-9]+$ ]] || stop "identifiant du système de la base illisible"
folder=$root/$system
# The archive is the database's: it reads it, then archives in it once
# restored. A folder of another owner (a copy made as root, an uid
# renumbered by another host) would hide its files as absent.
[[ -r $root && -x $root && -d $folder && -r $folder && -w $folder && -x $folder ]] ||
  stop "le dossier de l'archive de cette base, $folder, n'est pas à la base (uid 999, en 700) ; donnez-le-lui (chown -R 999:999)"

# Only <name>.<64 hex digits>.gz.age is this file's: another name may begin
# with this one's (a base backup's history begins with its segment's).
shopt -s nullglob
archived() {
  [[ ${1#"$folder/$2."} =~ ^[0-9a-f]{64}\.gz\.age$ ]]
}
found=()
for file in "$folder/$name".*.gz.age; do
  archived "$file" "$name" && found+=("$file")
done
if ((${#found[@]} == 0)); then
  # A segment absent while a later one of its timeline is archived: a hole,
  # where the replay would end.
  if [[ $name =~ ^[0-9A-F]{24}$ ]]; then
    for file in "$folder/${name:0:8}"*.gz.age; do
      later=${file#"$folder/"}
      later=${later:0:24}
      if [[ $later =~ ^[0-9A-F]{24}$ && $later > $name ]] && archived "$file" "$later"; then
        stop "absent de l'archive, qui tient pourtant $later, de la même ligne temporelle : il y manque"
      fi
    done
  fi
  # Not archived: the end of the archive, or a history that does not exist.
  exit 1
fi
((${#found[@]} == 1)) || stop "archivé plusieurs fois"
file=${found[0]}
sum=${file#"$folder/$name."}
sum=${sum%%.*}

# Written beside, then renamed once checked: PostgreSQL never reads a file
# half written nor one whose contents differ from those archived.
partial=$target.restoring
trap 'rm -f "$partial"' EXIT
age --decrypt --identity "$identity" "$file" 2>/dev/null | gunzip >"$partial" ||
  stop "il ne se déchiffre pas avec l'identité donnée, ou il est abîmé"
read -r got _ < <(sha256sum "$partial") || stop "impossible de le relire"
[[ $got == "$sum" ]] || stop "son contenu ne correspond pas à son nom"
mv -f "$partial" "$target" || stop "impossible de le remettre à PostgreSQL"
