#!/usr/bin/env bash
# Restores a backup of backup.sh (ADR-166) into the database of a new stack,
# still empty: its database service alone started (`docker compose up -d
# db`), before the migrations; a database that holds tables is refused. The
# identity (the age private key) opens the backup on the way in, and the
# dump is read in one transaction: all or nothing. Then `up -d` starts the
# stack, whose migrations apply what the backup lacks.
#
# The new stack keeps the backed-up stack's MIMORYA_MASTER_KEY: the secrets
# the database holds are sealed with it. Its database passwords may differ:
# roles are not part of a backup, the new stack's own are kept.
#   infra/backup/restore.sh backups/mimorya-….dump.age identity.txt
# The files are read from where the script was started; the stack's
# variables and compose files as for backup.sh, compose reading those from
# the repository's root. Needs age on the host. Its messages are for the
# operator, in French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la restauration : $1" >&2
  exit 1
}

(($# == 2)) || fail "usage : infra/backup/restore.sh <sauvegarde> <fichier d'identité age>"
file=$1
identity=$2
# Read from where the script was started, before it moves to the
# repository's root, where compose finds the stack.
[[ $file == /* ]] || file=$PWD/$file
[[ $identity == /* ]] || identity=$PWD/$identity
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

command -v age >/dev/null || fail "age n'est pas installé (https://age-encryption.org)"
[[ -r $file ]] || fail "impossible de lire $file"
[[ -r $identity ]] || fail "impossible de lire $identity"

# The database is ready once it listens over TCP: while it initialises (the
# application's role, the extensions), it answers on its socket only. What
# compose said last is shown if it never is: a variable left out, say.
ready=no
said=
for _ in $(seq 1 60); do
  if said=$(docker compose exec -T db pg_isready --host 127.0.0.1 --username mimorya 2>&1); then
    ready=yes
    break
  fi
  sleep 2
done
[[ $ready == yes ]] ||
  fail "la base de la pile n'est pas prête (docker compose up -d db) ; dernière réponse : $said"
tables=$(docker compose exec -T db psql --username mimorya --dbname mimorya -tAc \
  "select count(*) from pg_tables where schemaname = 'public'") ||
  fail "impossible de lire la base de la pile"
[[ $tables == 0 ]] ||
  fail "la base contient déjà $tables tables : restaurez seulement dans la base d'une pile nouvelle"

age --decrypt --identity "$identity" "$file" |
  docker compose exec -T db pg_restore --username mimorya --dbname mimorya \
    --single-transaction --exit-on-error ||
  fail "rien n'a été restauré (mauvaise identité, fichier abîmé, ou base qui l'a refusé)"
echo "Sauvegarde restaurée : $file ; démarrez maintenant la pile (up -d)"
