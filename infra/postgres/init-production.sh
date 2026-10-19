#!/usr/bin/env bash
# Executed once by the official postgres image when the data directory is
# created (compose.production.yml, ADR-034), after entrypoint-production.sh
# checked the passwords: the application's role, with the password chosen at
# installation, and the extensions the migrations need. The password is read
# from the environment by psql itself, never passed as an argument (other
# accounts of the host could read it in the process list).
set -euo pipefail

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
\getenv app_password MIMORYA_APP_DB_PASSWORD
CREATE ROLE mimorya_app LOGIN PASSWORD :'app_password' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS vector;
SQL
