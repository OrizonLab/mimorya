#!/usr/bin/env bash
# Entry point of the production database (compose.production.yml, ADR-034):
# checks the chosen passwords before the image's own entry point creates
# anything. The passwords go in connection URLs: letters and digits only, 16
# at least (`openssl rand -hex 32`). A refused password leaves no data
# directory behind: the container fails with the same message at each
# restart until the password is fixed.
set -euo pipefail

for name in POSTGRES_PASSWORD MIMORYA_APP_DB_PASSWORD; do
  value=${!name:-}
  if [[ ${#value} -lt 16 || $value == *[!A-Za-z0-9]* ]]; then
    echo "$name: at least 16 letters and digits are required (it goes in connection URLs)" >&2
    exit 1
  fi
done

exec docker-entrypoint.sh "$@"
