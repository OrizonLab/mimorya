#!/usr/bin/env bash
# Checks the operator commands of the API image (ADR-033) on the running
# demonstration stack: the migrations apply again without change, a real
# company is created next to the demo one, and its first administrator's
# invitation link opens through the reverse proxy.
#   DEMO_PASSWORD=… infra/docker/operator-test.sh [https://localhost]
set -euo pipefail

base=${1:-https://localhost}
compose=(docker compose -f compose.demo.yml)
fail() {
  echo "operator test failed: $1" >&2
  exit 1
}

# The owner's connection is the seed service's, which applied them first.
"${compose[@]}" run --rm --no-deps seed mimorya-admin migrate >/dev/null ||
  fail "migrations not applied"

output=$("${compose[@]}" exec -T api mimorya-admin create-company \
  --name 'Entreprise de vérification' --slug verification \
  --admin-email admin@verification.example --admin-name 'Administrateur de vérification') ||
  fail "company not created"
link=$(grep -o "$base/invitation#[A-Za-z0-9_-]*" <<<"$output") ||
  fail "no invitation link leading to $base"
token=${link#*#}

shown=$(curl -sk --max-time 10 -X POST "$base/api/auth/invitation" \
  -H 'content-type: application/json' -H 'x-mimorya-request: 1' -H "origin: $base" \
  --data "{\"token\":\"$token\"}")
grep -q '"company":"Entreprise de vérification"' <<<"$shown" || fail "invitation not opened: $shown"

echo "operator test passed: $base"
