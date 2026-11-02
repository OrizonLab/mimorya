#!/usr/bin/env bash
# Backs up the production stack's database (ADR-166): a dump in PostgreSQL's
# custom format, read from the database's container and encrypted on its way
# out with age, for the recipients given. Recipients are public keys: the
# identity that opens a backup stays off the server, which cannot read its
# own backups. Nothing unencrypted touches the disk; the file is readable by
# its owner only, and appears complete or not at all.
#   MIMORYA_BACKUP_RECIPIENTS='age1…' infra/backup/backup.sh [file]
#   MIMORYA_BACKUP_RECIPIENTS_FILE=recipients.txt infra/backup/backup.sh [file]
# The file defaults to backups/mimorya-<UTC time>.dump.age, from the
# repository's root, where it runs from wherever started. The stack's
# variables come from the environment (or COMPOSE_ENV_FILES), its compose
# files from COMPOSE_FILE (compose.production.yml by default). Needs age
# (https://age-encryption.org) on the host.
set -euo pipefail
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

fail() {
  echo "backup failed: $1" >&2
  exit 1
}

command -v age >/dev/null || fail "age is not installed (https://age-encryption.org)"
recipients=()
for recipient in ${MIMORYA_BACKUP_RECIPIENTS:-}; do
  recipients+=(--recipient "$recipient")
done
if [[ -n ${MIMORYA_BACKUP_RECIPIENTS_FILE:-} ]]; then
  [[ -r $MIMORYA_BACKUP_RECIPIENTS_FILE ]] ||
    fail "cannot read $MIMORYA_BACKUP_RECIPIENTS_FILE"
  recipients+=(--recipients-file "$MIMORYA_BACKUP_RECIPIENTS_FILE")
fi
((${#recipients[@]})) ||
  fail "no recipient: set MIMORYA_BACKUP_RECIPIENTS (age public keys) or MIMORYA_BACKUP_RECIPIENTS_FILE"

file=${1:-backups/mimorya-$(date -u +%Y%m%dT%H%M%SZ).dump.age}
[[ ! -e $file ]] || fail "$file already exists"
mkdir -p "$(dirname "$file")"
umask 077
partial="$file.partial"
trap 'rm -f "$partial"' EXIT
docker compose exec -T db pg_dump --username mimorya --dbname mimorya --format custom |
  age "${recipients[@]}" --output "$partial" ||
  fail "the dump or its encryption failed: nothing was kept"
mv "$partial" "$file"
trap - EXIT
echo "backup written: $file"
