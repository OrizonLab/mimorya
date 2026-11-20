#!/usr/bin/env bash
# Restores a base backup of backup.sh --base (ADR-206) into the database of a
# new stack, then replays the archived WAL up to a chosen instant (ADR-214,
# chantier 4, lot 70b): a restore to a point in time. The new stack's
# database must be stopped and empty (never started, or `down -v`), its
# archive's folder (MIMORYA_WAL_ARCHIVE) holding a copy of the backed-up
# database's archive, as the operator brings it back from off site.
#
# The identity (the age private key) opens the base on its way in, and the
# archived WAL during the recovery: it is handed to the database's container
# on its standard input and kept in its memory (/dev/shm) for the recovery
# only, never on a volume nor in its environment. The database starts in
# recovery: it replays the WAL from the archive (infra/postgres/restore-wal.sh)
# up to the instant, then ends its recovery on a new timeline, beyond those
# already archived, so that it archives beside the backed-up database's files,
# never over them. Without an instant, all the WAL archived is replayed. The
# container of the recovery is then stopped; `up -d` starts the stack.
#
# The new stack keeps the backed-up stack's MIMORYA_MASTER_KEY, which seals
# the secrets the database holds, and its passwords: the roles are part of a
# base backup. A restore that fails leaves the database's files as they were
# unpacked: `docker compose down -v` empties it before trying again.
#   infra/backup/restore-base.sh backups/mimorya-….base.tar.gz.age identity.txt ['2026-11-20 14:30:00+01']
# The instant is PostgreSQL's (timestamptz): the restore stops before the
# first transaction committed after it. The history replayed is the base's
# own timeline (MIMORYA_RESTORE_TIMELINE, `current` by default), never one a
# former restore of the same archive began. The files are read from where the
# script was started; the stack's variables and compose files as for
# backup.sh. Needs age on the host. Its messages are for the operator, in
# French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la restauration à un instant : $1" >&2
  exit 1
}

(($# == 2 || $# == 3)) ||
  fail "usage : infra/backup/restore-base.sh <sauvegarde de base> <fichier d'identité age> [instant]"
file=$1
identity=$2
until=${3:-}
[[ $file == /* ]] || file=$PWD/$file
[[ $identity == /* ]] || identity=$PWD/$identity
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

command -v age >/dev/null || fail "age n'est pas installé (https://age-encryption.org)"
[[ -r $file ]] || fail "impossible de lire $file"
[[ -r $identity ]] || fail "impossible de lire $identity"
# A timestamp with its zone, as PostgreSQL reads it: digits, separators and
# letters only, never a quote that would end the setting.
[[ -z $until || $until =~ ^[0-9A-Za-z\ :.+/-]+$ ]] || fail "instant illisible : $until"
timeout=${MIMORYA_RESTORE_TIMEOUT:-3600}
[[ $timeout =~ ^[0-9]+$ ]] || fail "MIMORYA_RESTORE_TIMEOUT : un nombre de secondes"
# The history replayed: the base's own timeline by default, never one a
# former restore of this archive started (`latest` would follow it);
# another when the backed-up database itself changed timeline after the
# base (a former restore put in production), by its number.
timeline_wanted=${MIMORYA_RESTORE_TIMELINE:-current}
[[ $timeline_wanted =~ ^(current|latest|[0-9]+)$ ]] ||
  fail "MIMORYA_RESTORE_TIMELINE : current, latest ou un numéro de ligne temporelle"

running=$(docker compose ps --status running --services 2>/dev/null) || fail "impossible de lire l'état de la pile"
! grep -qx db <<<"$running" ||
  fail "la base de la pile est démarrée : restaurez seulement dans la base arrêtée d'une pile nouvelle"
in_db() {
  docker compose run --rm --no-deps -T --entrypoint bash db -c "$1"
}
in_db 'shopt -s nullglob dotglob; files=(/var/lib/postgresql/data/*); ((${#files[@]} == 0))' ||
  fail "la base contient déjà des fichiers : restaurez seulement dans la base vide d'une pile nouvelle (down -v)"

# Unpacked as root, then given to the database: its folder its own alone.
age --decrypt --identity "$identity" "$file" |
  in_db 'set -e; cd /var/lib/postgresql/data; gunzip | tar -x --no-same-owner
    chown -R postgres:postgres .; chmod 700 .' ||
  fail "rien n'a été restauré (mauvaise identité ou fichier abîmé) ; videz la base (down -v) avant de réessayer"
system=$(in_db 'LC_ALL=C pg_controldata /var/lib/postgresql/data |
  sed -n "s/^Database system identifier: *//p"') || true
[[ $system =~ ^[0-9]+$ ]] || fail "la sauvegarde ne contient pas de base lisible ; videz la base (down -v)"
in_db "test -d /var/lib/postgresql/wal-archive/$system" ||
  fail "l'archive des WAL de cette base ($system) n'est pas dans le dossier de l'archive de la pile ; videz la base (down -v)"

# The recovery: its settings on the command line, never kept in the
# database's configuration; the identity from the standard input into the
# container's memory, the database's own.
settings=(
  -c 'restore_command=bash /usr/local/bin/restore-wal.sh %f %p'
  -c recovery_target_action=promote
  -c "recovery_target_timeline=$timeline_wanted"
)
[[ -z $until ]] || settings+=(-c "recovery_target_time=$until")
container=mimorya-restore-$$
log=$(mktemp)
trap 'docker rm -f "$container" >/dev/null 2>&1 || true; rm -f "$log"' EXIT
docker compose run --rm --no-deps -T --name "$container" --entrypoint bash db -c '
  set -e
  install -d -m 700 -o postgres -g postgres /dev/shm/mimorya-restore
  install -m 600 -o postgres -g postgres /dev/stdin /dev/shm/mimorya-restore/identity
  install -m 600 -o postgres -g postgres /dev/null /var/lib/postgresql/data/recovery.signal
  exec bash /usr/local/bin/entrypoint-production.sh postgres "$@"' restore "${settings[@]}" \
  <"$identity" >"$log" 2>&1 &

# Waits for the recovery to end: the database out of recovery, on its new
# timeline; a recovery that stops (an instant before the base's end, an
# archive missing or unreadable) is said with the end of its log.
state=
for _ in $(seq 1 "$timeout"); do
  state=$(docker exec "$container" psql --username mimorya --dbname mimorya -tAc \
    'select pg_is_in_recovery()' 2>/dev/null) || state=
  [[ $state == f ]] && break
  if [[ $(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null) != true ]] &&
    ! jobs -r | grep -q .; then
    tail -n 20 "$log" >&2
    fail "la récupération s'est arrêtée ; videz la base (down -v) avant de réessayer"
  fi
  sleep 1
done
[[ $state == f ]] || {
  tail -n 20 "$log" >&2
  fail "la récupération n'a pas fini en $timeout s (MIMORYA_RESTORE_TIMEOUT)"
}
timeline=$(docker exec "$container" psql --username mimorya --dbname mimorya -tAc \
  "select substr(pg_walfile_name(pg_current_wal_lsn()), 1, 8)") || timeline=?
docker stop "$container" >/dev/null || fail "la base restaurée ne s'est pas arrêtée"
wait || true
point=
[[ -z $until ]] || point=" jusqu'à $until"
echo "Base restaurée$point, ligne temporelle $timeline ; démarrez maintenant la pile (up -d)"
