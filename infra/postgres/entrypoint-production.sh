#!/usr/bin/env bash
# Entry point of the production database (compose.production.yml, ADR-034):
# checks the chosen passwords before the image's own entry point creates
# anything. The passwords go in connection URLs: letters and digits only, 16
# at least (`openssl rand -hex 32`). A refused password leaves no data
# directory behind: the container fails with the same message at each
# restart until the password is fixed.
#
# With the backups' recipients given (MIMORYA_BACKUP_RECIPIENTS, ADR-166),
# every WAL segment is archived, encrypted for them (ADR-206): they are
# checked by age before anything is created or changed, as the passwords
# are, and the archive's folder is made the database's own; it must take
# hard links (ADR-215), which archive-wal.sh names each file by, before the
# database is created. PostgreSQL then
# starts with its archive command and a segment switched at least every five
# minutes, from the command line, which the settings of ALTER SYSTEM do not
# override.
set -euo pipefail

for name in POSTGRES_PASSWORD MIMORYA_APP_DB_PASSWORD; do
  value=${!name:-}
  if [[ ${#value} -lt 16 || $value == *[!A-Za-z0-9]* ]]; then
    echo "$name: at least 16 letters and digits are required (it goes in connection URLs)" >&2
    exit 1
  fi
done

archiving=()
if [[ -n ${MIMORYA_BACKUP_RECIPIENTS:-} ]]; then
  # Separated by spaces, tabs or new lines, as backup.sh reads them.
  read -ra keys <<<"${MIMORYA_BACKUP_RECIPIENTS//$'\n'/ }"
  recipients=()
  for key in "${keys[@]}"; do
    recipients+=(--recipient "$key")
  done
  if ! said=$(age "${recipients[@]}" </dev/null 2>&1 >/dev/null); then
    echo "MIMORYA_BACKUP_RECIPIENTS : destinataires refusés par age, rien n'est archivé (${said%%$'\n'*})" >&2
    exit 1
  fi
  archive=/var/lib/postgresql/wal-archive
  if ! said=$(chown postgres:postgres "$archive" 2>&1 && chmod 700 "$archive" 2>&1); then
    echo "Archive des WAL : le dossier monté sur $archive est inutilisable ($said)" >&2
    exit 1
  fi
  # A folder that takes no hard links (FAT, exFAT, some CIFS or FUSE
  # mounts) would fail every archiving while the database runs: tried once,
  # as the database, before anything is created.
  probe=$archive/.mimorya-link-probe-$$
  if ! said=$(gosu postgres touch "$probe" 2>&1); then
    echo "Archive des WAL : la base ne peut écrire dans le dossier monté sur $archive, rien n'y serait archivé (${said%%$'\n'*})" >&2
    exit 1
  fi
  said=$(gosu postgres ln "$probe" "$probe-link" 2>&1) || linked=no
  rm -f "$probe" "$probe-link"
  if [[ ${linked:-yes} == no ]]; then
    echo "Archive des WAL : le dossier monté sur $archive n'accepte pas les liens physiques, rien n'y serait archivé (${said%%$'\n'*})" >&2
    exit 1
  fi
  archiving=(
    -c archive_mode=on
    -c 'archive_command=bash /usr/local/bin/archive-wal.sh %p %f'
    -c archive_timeout=300
  )
fi

exec docker-entrypoint.sh "$@" "${archiving[@]}"
