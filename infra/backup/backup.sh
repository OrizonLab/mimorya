#!/usr/bin/env bash
# Backs up the production stack's database (ADR-166): a dump in PostgreSQL's
# custom format, read from the database's container and encrypted on its way
# out with age, for the recipients given. Recipients are public keys: the
# identity that opens a backup stays off the server, which cannot read its
# own backups. Nothing unencrypted touches the disk; the file and its folder
# are readable by their owner only, and the file appears complete or not at
# all.
#   MIMORYA_BACKUP_RECIPIENTS='age1…' infra/backup/backup.sh [file]
#   MIMORYA_BACKUP_RECIPIENTS_FILE=recipients.txt infra/backup/backup.sh [file]
# The file defaults to backups/mimorya-<UTC time>.dump.age, in the
# repository's root; a file or recipients file given is read from where the
# script was started. The stack's variables come from the environment (or
# COMPOSE_ENV_FILES), its compose files from COMPOSE_FILE
# (compose.production.yml by default). Needs age (https://age-encryption.org)
# on the host. Its messages are for the operator, in French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la sauvegarde : $1" >&2
  exit 1
}

# Paths given are read from where the script was started, before it moves to
# the repository's root, where compose finds the stack.
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

file=${file:-backups/mimorya-$(date -u +%Y%m%dT%H%M%SZ).dump.age}
[[ ! -e $file ]] || fail "$file existe déjà"
umask 077
mkdir -p "$(dirname "$file")"
partial="$file.partial"
trap 'rm -f "$partial"' EXIT
docker compose exec -T db pg_dump --username mimorya --dbname mimorya --format custom |
  age "${recipients[@]}" --output "$partial" ||
  fail "le dump ou son chiffrement a échoué : rien n'a été gardé"
mv "$partial" "$file"
trap - EXIT
echo "Sauvegarde écrite : $file"
