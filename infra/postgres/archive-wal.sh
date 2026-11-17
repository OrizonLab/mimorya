#!/usr/bin/env bash
# PostgreSQL's archive command in the production stack (ADR-206), run by the
# archiver as postgres from the data directory: archive-wal.sh %p %f. Each
# file PostgreSQL hands over (a WAL segment, a timeline's history, a base
# backup's history) is compressed, then encrypted with age for the backups'
# recipients (MIMORYA_BACKUP_RECIPIENTS: public keys, so that the server
# cannot read its own archive), into
#   /var/lib/postgresql/wal-archive/<system identifier>/<name>.<sha256>.gz.age
# One folder per database system: a database made anew (a dump restored
# into a new stack) archives beside the former one's files, never over them.
# The name holds the SHA-256 of the file as PostgreSQL wrote it: handed over
# again with the same contents (the archiver stopped before noting its
# success), the file is there already and the command succeeds; with other
# contents, it is refused and nothing is replaced. A file appears whole or
# not at all, on disk before its success is said. Its messages go to
# PostgreSQL's log, for the operator, in French (ADR-013).
set -euo pipefail

wal=${1:-}
name=${2:-}
root=/var/lib/postgresql/wal-archive

fail() {
  echo "Archivage du WAL ${name:-?} refusé : $1" >&2
  exit 1
}

# What PostgreSQL archives: segments, the partial segment of a timeline left
# at a promotion, base backups' and timelines' histories.
[[ $name =~ ^[0-9A-F]{24}(\.partial|\.[0-9A-F]{8}\.backup)?$|^[0-9A-F]{8}\.history$ ]] ||
  fail "nom inattendu"
given=${MIMORYA_BACKUP_RECIPIENTS:-}
read -ra keys <<<"${given//$'\n'/ }"
((${#keys[@]})) || fail "aucun destinataire (MIMORYA_BACKUP_RECIPIENTS)"
recipients=()
for key in "${keys[@]}"; do
  recipients+=(--recipient "$key")
done

system=$(LC_ALL=C pg_controldata . | sed -n 's/^Database system identifier: *//p') || true
[[ $system =~ ^[0-9]+$ ]] || fail "identifiant du système de la base illisible"
folder=$root/$system
mkdir -p -m 700 "$folder" || fail "impossible de créer $folder"
sum=$(sha256sum <"$wal") || fail "impossible de lire $wal"
file=$folder/$name.${sum%% *}.gz.age

# Archived already: with these contents, done; with others, refused. Another
# file's name may begin with this one's (a base backup's history begins with
# its segment's): only <name>.<64 hex digits>.gz.age is this file's.
shopt -s nullglob
for found in "$folder/$name".*.gz.age; do
  [[ ${found#"$folder/$name."} =~ ^[0-9a-f]{64}\.gz\.age$ ]] || continue
  if [[ $found == "$file" ]]; then
    sync "$folder" || fail "impossible d'écrire $folder sur le disque"
    exit 0
  fi
  fail "déjà archivé avec un autre contenu (${found##*/}) : rien n'est remplacé"
done

# Written beside under a hidden name, readable by its owner only, then named
# by a hard link, which never replaces anything. A partial file left by an
# archiving killed midway is removed first: the archiver runs one at a time.
umask 077
rm -f "$folder/.$name.partial-"*
partial=
trap 'rm -f ${partial:+"$partial"}' EXIT
partial=$(mktemp "$folder/.$name.partial-XXXXXX") || fail "impossible d'écrire dans $folder"
gzip -c <"$wal" | age "${recipients[@]}" --output "$partial" ||
  fail "la compression ou le chiffrement a échoué"
sync "$partial" || fail "impossible d'écrire $partial sur le disque"
if ! said=$(ln -T "$partial" "$file" 2>&1); then
  [[ -e $file ]] || fail "impossible de nommer ${file##*/} : $said"
fi
sync "$folder" || fail "impossible d'écrire $folder sur le disque"
