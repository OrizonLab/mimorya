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
# up to the instant and pauses there. The instant is checked against the
# base's start, as PostgreSQL reads both: a base begun after it cannot bring
# the database back to it. Only then does the recovery end, on a new
# timeline, beyond those already archived, so that it archives beside the
# backed-up database's files, never over them. Without an instant, all the
# WAL archived is replayed. The container of the recovery is then stopped;
# `up -d` starts the stack.
#
# The new stack keeps the backed-up stack's MIMORYA_MASTER_KEY, which seals
# the secrets the database holds, and its passwords: the roles are part of a
# base backup. A restore that fails leaves the database's files as they were
# unpacked: `docker compose down -v` empties it before trying again.
#   infra/backup/restore-base.sh backups/mimorya-….base.tar.gz.age identity.txt ['2026-11-20 14:30:00+01']
# The instant is PostgreSQL's (timestamptz): the restore stops before the
# first transaction committed after it; it must be a second at least after
# the base's start (its label's START TIME). The history replayed is the
# base's own timeline (MIMORYA_RESTORE_TIMELINE, `current` by default), never
# one a former restore of the same archive began. The files are read from where the
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
[[ $timeout =~ ^[1-9][0-9]*$ ]] || fail "MIMORYA_RESTORE_TIMEOUT : un nombre de secondes"
# The history replayed: the base's own timeline by default; another when the
# backed-up database itself changed timeline after the base (a former
# restore put in production), by its number. Never `latest`: it would follow
# the newest timeline of the archive, one a former restore of it began, and
# bring the database back to that restore's instant without a word.
timeline_wanted=${MIMORYA_RESTORE_TIMELINE:-current}
[[ $timeline_wanted =~ ^(current|[1-9][0-9]*)$ ]] ||
  fail "MIMORYA_RESTORE_TIMELINE : current ou le numéro d'une ligne temporelle"

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
# When the base began, as its label says (UTC, to the second): PostgreSQL
# reads the label at the start of the recovery, and renames it.
started=$(in_db 'sed -n "s/^START TIME: //p" /var/lib/postgresql/data/backup_label') || true
[[ $started =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}\ [0-9]{2}:[0-9]{2}:[0-9]{2}\ [A-Za-z0-9+-]+$ ]] ||
  fail "la sauvegarde n'a pas d'étiquette lisible (backup_label) ; videz la base (down -v)"

# The recovery: its settings on the command line, never kept in the
# database's configuration; the identity from the standard input into the
# container's memory, the database's own. With an instant, it pauses there,
# open to reading (hot standby), until the instant is checked.
settings=(
  -c 'restore_command=bash /usr/local/bin/restore-wal.sh %f %p'
  -c "recovery_target_timeline=$timeline_wanted"
)
[[ -z $until ]] || settings+=(
  -c "recovery_target_time=$until"
  -c recovery_target_action=pause
  -c hot_standby=on
)
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

in_restore() {
  docker exec "$container" psql --username mimorya --dbname mimorya -tAc "$1" 2>/dev/null
}
# Waits for the recovery to end: the database out of recovery, on its new
# timeline. Paused at the instant, the instant is checked against the base's
# start before the recovery goes on: a base begun after it holds what was
# committed after it, which the recovery cannot undo; it would end at the
# first commit past the base, without a word. A recovery that stops (an
# instant before the base's end with a commit after it there, an instant
# past the archive, a hole or an unreadable file) is said with the end of
# its log.
state=
for _ in $(seq 1 "$timeout"); do
  state=$(in_restore "select case when not pg_is_in_recovery() then 'promoted'
    when pg_get_wal_replay_pause_state() = 'paused' then 'paused' end") || state=
  [[ $state == promoted ]] && break
  if [[ $state == paused ]]; then
    after=$(in_restore "select '$until'::timestamptz >= '$started'::timestamptz + interval '1 second'") ||
      after=
    if [[ $after != t ]]; then
      docker stop --time "$timeout" "$container" >/dev/null 2>&1 || true
      fail "l'instant ($until) ne vient pas au moins une seconde après le début de la sauvegarde de base ($started) : prenez une sauvegarde plus ancienne ; videz la base (down -v) avant de réessayer"
    fi
    in_restore 'select pg_wal_replay_resume()' >/dev/null || fail "la récupération n'a pas repris après l'instant"
  fi
  if [[ $(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null) != true ]] &&
    ! jobs -r | grep -q .; then
    tail -n 20 "$log" >&2
    fail "la récupération s'est arrêtée ; videz la base (down -v) avant de réessayer"
  fi
  sleep 1
done
[[ $state == promoted ]] || {
  tail -n 20 "$log" >&2
  fail "la récupération n'a pas fini en $timeout s (MIMORYA_RESTORE_TIMEOUT), elle est arrêtée ; videz la base (down -v) avant de réessayer"
}
timeline=$(in_restore "select substr(pg_walfile_name(pg_current_wal_lsn()), 1, 8)") || timeline=?
# Its shutdown writes what the replay left in memory: as long as it needs.
docker stop --time "$timeout" "$container" >/dev/null || fail "la base restaurée ne s'est pas arrêtée"
wait || true
point=
[[ -z $until ]] || point=" jusqu'à $until"
echo "Base restaurée$point, ligne temporelle $timeline ; démarrez maintenant la pile (up -d)"
