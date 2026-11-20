#!/usr/bin/env bash
# The WAL archive's state (ADR-215, chantier 4, lot 70c), for the operator's
# monitoring (cron, supervision): it says where the archive stands and exits
# with 1 when it needs a look, 0 otherwise:
#   infra/backup/archive-status.sh [--base-within <hours>]
# Needing a look:
#   - archiving off (no MIMORYA_BACKUP_RECIPIENTS given to the database);
#   - the last archiving failed (pg_stat_archiver): the database keeps its
#     WAL and tries again, at the cost of its own disk;
#   - a WAL waiting to be archived for more than ten minutes, when a
#     segment is closed every five at most: archiving is late;
#   - with --base-within, no base backup archived (its history) in the last
#     <hours> hours, or none at all.
# It reads the running database (pg_stat_archiver, its archive status) and
# the archive of its system, in its container. The stack's variables and
# compose files as for backup.sh. Its messages are for the operator, in
# French (ADR-013).
set -euo pipefail

fail() {
  echo "Échec de la lecture de l'archive : $1" >&2
  exit 2
}

within=
if (($#)); then
  [[ $# == 2 && $1 == --base-within && $2 =~ ^[1-9][0-9]{0,4}$ ]] ||
    fail "usage : infra/backup/archive-status.sh [--base-within <heures>]"
  within=$2
fi
cd "$(dirname "$0")/../.."
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}

state=$(docker compose exec -T db psql --username mimorya --dbname mimorya -tA -F ' ' -c "
  select current_setting('archive_mode'),
         coalesce(last_archived_wal, '-'),
         coalesce(extract(epoch from last_archived_time)::bigint, 0),
         coalesce(last_failed_wal, '-'),
         coalesce(extract(epoch from last_failed_time)::bigint, 0),
         failed_count,
         (select count(*) from pg_ls_archive_statusdir() where name like '%.ready'),
         coalesce((select extract(epoch from min(modification))::bigint
                   from pg_ls_archive_statusdir() where name like '%.ready'), 0),
         extract(epoch from now())::bigint,
         (select system_identifier from pg_control_system())
  from pg_stat_archiver") || fail "la base de la pile ne répond pas"
read -r mode archived archived_at failed failed_at failures waiting waiting_since now system <<<"$state"
[[ $system =~ ^[0-9]+$ ]] || fail "réponse illisible : $state"

# Minutes or hours since $1, in words.
since() {
  local minutes=$(((now - $1) / 60))
  ((minutes < 120)) && echo "$minutes min" || echo "$((minutes / 60)) h"
}

problems=()
if [[ $mode == off ]]; then
  problems+=("l'archivage des WAL est désactivé : aucun destinataire (MIMORYA_BACKUP_RECIPIENTS) n'est donné à la base")
else
  ((failed_at <= archived_at)) ||
    problems+=("le dernier archivage a échoué (WAL $failed, il y a $(since "$failed_at")) : voyez le journal de la base")
  ((waiting == 0 || now - waiting_since <= 600)) ||
    problems+=("$waiting WAL attendent l'archivage, le plus ancien depuis $(since "$waiting_since")")
fi

if [[ $mode != off ]]; then
  # The newest base's history archived, as the database sees its archive.
  base_at=$(docker compose exec -T -u postgres db find "/var/lib/postgresql/wal-archive/$system" -maxdepth 1 \
    -regextype posix-extended -regex '.*/[0-9A-F]{24}\.[0-9A-F]{8}\.backup\.[0-9a-f]{64}\.gz\.age' \
    -printf '%T@\n' 2>/dev/null | sort -n | tail -n 1) || base_at=
  base_at=${base_at%.*}
fi
if [[ -n ${base_at:-} ]]; then
  base="dernière sauvegarde de base archivée il y a $(since "$base_at")"
  [[ -z $within ]] || ((now - base_at <= within * 3600)) ||
    problems+=("aucune sauvegarde de base archivée depuis $within h (la dernière il y a $(since "$base_at"))")
else
  base="aucune sauvegarde de base archivée"
  [[ -z $within ]] || problems+=("aucune sauvegarde de base archivée")
fi

if ((${#problems[@]})); then
  for problem in "${problems[@]}"; do
    echo "Archive à vérifier : $problem" >&2
  done
  exit 1
fi
last="aucun WAL archivé depuis le démarrage de la base"
[[ $archived == - ]] || last="dernier WAL archivé $archived il y a $(since "$archived_at")"
echo "Archive à jour : $last, $failures échec(s) depuis le démarrage ; $base"
