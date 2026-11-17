#!/usr/bin/env bash
# Backs up the production stack's database (ADR-166): a dump in PostgreSQL's
# custom format, read from the database's container and encrypted on its way
# out with age, for the recipients given. Recipients are public keys: the
# identity that opens a backup stays off the server, which cannot read its
# own backups. Nothing unencrypted touches the disk; the file, and the
# folders the script creates for it, are readable by their owner only, and
# the file appears complete or not at all. It is never replaced: of two runs
# naming the same file, one writes it and the other is refused. Its folder
# must take hard links, as ext4, XFS, Btrfs, ZFS and NFS do and FAT, exFAT
# and some FUSE mounts do not: that is tried before the dump.
#   MIMORYA_BACKUP_RECIPIENTS='age1…' infra/backup/backup.sh [--base] [file]
#   MIMORYA_BACKUP_RECIPIENTS_FILE=recipients.txt infra/backup/backup.sh [--base] [file]
# With --base, the backup is the base of a restore to a point in time
# (ADR-206) rather than a dump: the database's files copied by
# pg_basebackup, as a tar with the WAL it needs, compressed, then encrypted
# the same way; the WAL written after it is in the archive the database
# keeps (infra/postgres/archive-wal.sh). The file defaults to
# backups/mimorya-<UTC time>.dump.age (.base.tar.gz.age with --base), in the
# repository's root; a file or recipients file given is read from where the
# script was started. The stack's variables come from the environment (or
# COMPOSE_ENV_FILES), its compose files from COMPOSE_FILE
# (compose.production.yml by default): compose reads those from the
# repository's root, where it runs. Needs age (https://age-encryption.org)
# on the host. Its messages are for the operator, in French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la sauvegarde : $1" >&2
  exit 1
}

# Paths given are read from where the script was started, before it moves to
# the repository's root, where compose finds the stack.
kind=dump
if [[ ${1:-} == --base ]]; then
  kind=base
  shift
fi
[[ ${1:-} != -* ]] || fail "option inconnue : $1 (seule --base est reconnue)"
(($# <= 1)) || fail "un seul fichier attendu, $# donnés"
file=${1:-}
[[ -z $file || $file == /* ]] || file=$PWD/$file
recipients_file=${MIMORYA_BACKUP_RECIPIENTS_FILE:-}
[[ -z $recipients_file || $recipients_file == /* ]] || recipients_file=$PWD/$recipients_file
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

command -v age >/dev/null || fail "age n'est pas installé (https://age-encryption.org)"
recipients=()
for recipient in ${MIMORYA_BACKUP_RECIPIENTS:-}; do
  recipients+=(--recipient "$recipient")
done
if [[ -n $recipients_file ]]; then
  [[ -r $recipients_file ]] || fail "impossible de lire $recipients_file"
  recipients+=(--recipients-file "$recipients_file")
fi
((${#recipients[@]})) ||
  fail "aucun destinataire : renseignez MIMORYA_BACKUP_RECIPIENTS (clés publiques age) ou MIMORYA_BACKUP_RECIPIENTS_FILE"

suffix=.dump.age
[[ $kind == dump ]] || suffix=.base.tar.gz.age
file=${file:-$PWD/backups/mimorya-$(date -u +%Y%m%dT%H%M%SZ)$suffix}
# Anything at that name, a dangling symbolic link included, is in the way.
taken() { [[ -e $file || -L $file ]]; }
! taken || fail "$file existe déjà"
umask 077
mkdir -p "$(dirname "$file")" || fail "impossible de créer le dossier de $file"
# Each run writes its own partial file, made by mktemp: readable by its
# owner only from its creation, never one found there nor another run's.
partial=
probe=
trap 'rm -f ${partial:+"$partial"} ${probe:+"$probe"} 2>/dev/null' EXIT
partial=$(mktemp "$file.partial.XXXXXX") || fail "impossible d'écrire à côté de $file"
# The backup is named by a hard link: tried first, so that a folder which
# takes none is said before the dump rather than after it, with the reason
# ln gives, which may be another. The probe's name is as long as the partial
# file's, so that a name the backup can take is not refused for its probe's,
# and of a template of its own, so that it is never another run's partial
# file. The trap removes it only while it is this run's: a failed ln made
# none.
probe=$(mktemp -u "$file.probe.XXXXXXXX")
if ! said=$(ln -T "$partial" "$probe" 2>&1); then
  probe=
  fail "lien physique impossible à côté de $file : $said. Son dossier doit accepter les liens physiques : ext4, XFS, Btrfs, ZFS ou NFS conviennent ; FAT, exFAT et certains montages FUSE, non"
fi
rm -f "$probe"
probe=
if [[ $kind == dump ]]; then
  docker compose exec -T db pg_dump --username mimorya --dbname mimorya --format custom |
    age "${recipients[@]}" --output "$partial" ||
    fail "le dump ou son chiffrement a échoué : rien n'a été gardé"
else
  # The WAL from the backup's start to its end is fetched into it, so that
  # it restores on its own too; the checkpoint it starts from is made at once.
  docker compose exec -T db pg_basebackup --username mimorya --pgdata - --format tar \
    --wal-method fetch --checkpoint fast |
    gzip | age "${recipients[@]}" --output "$partial" ||
    fail "la copie des fichiers de la base ou son chiffrement a échoué : rien n'a été gardé"
fi
# The hard link names the backup whole, and fails rather than replace
# anything: another run's backup, written meanwhile under the same name, is
# kept; with -T, a folder made at that name is not linked into either.
if ! said=$(ln -T "$partial" "$file" 2>&1); then
  ! taken ||
    fail "$file existe déjà : il a été créé pendant celle-ci, qui n'est pas gardée"
  fail "impossible de nommer la sauvegarde $file : $said"
fi
echo "Sauvegarde écrite : $file"
