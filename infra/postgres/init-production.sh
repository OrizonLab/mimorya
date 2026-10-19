#!/usr/bin/env bash
# Executed once by the official postgres image when the data directory is
# created (compose.production.yml, ADR-034): the application's role, with the
# password chosen at installation, and the extensions the migrations need.
# The passwords go in connection URLs: letters and digits only, 16 at least
# (`openssl rand -hex 32`). If this script fails, the data directory is
# already created: remove the volume before starting again.
set -euo pipefail

for name in POSTGRES_PASSWORD MIMORYA_APP_DB_PASSWORD; do
  value=${!name:-}
  if [[ ${#value} -lt 16 || $value == *[!A-Za-z0-9]* ]]; then
    echo "$name: at least 16 letters and digits are required (it goes in connection URLs)" >&2
    exit 1
  fi
done

psql -v ON_ERROR_STOP=1 -v app_password="$MIMORYA_APP_DB_PASSWORD" \
  --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
CREATE ROLE mimorya_app LOGIN PASSWORD :'app_password' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS vector;
SQL
