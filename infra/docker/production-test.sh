#!/usr/bin/env bash
# Checks a freshly installed production stack (ADR-034) through its reverse
# proxy, as an installation starts: HTTPS with HSTS, the page policy and the
# API answer; one API process and one worker, each in its role, then two
# workers side by side (ADR-086, ADR-087); the API, the workers and the web
# application do not run as root; the migrations apply again without
# change; no demonstration company exists and the sign-in page lists no demo
# account; the operator creates a company (ADR-033), whose first
# administrator opens the invitation link, chooses a password and gets a
# secure session in a company that is not a demo.
#
# It leaves that company and its active administrator behind: for a stack
# thrown away afterwards only (the CI's), hence DISPOSABLE_STACK=yes. The
# stack's variables come from the environment (or COMPOSE_ENV_FILES), its
# compose files from COMPOSE_FILE (compose.production.yml by default; the CI
# adds compose.production.build.yml, which builds the images).
#   DISPOSABLE_STACK=yes infra/docker/production-test.sh [https://localhost]
set -euo pipefail
source "$(dirname "$0")/checks.sh"

check_name="production test"
base=${1:-https://localhost}
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
compose=(docker compose)
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it leaves a company and an administrator behind: set DISPOSABLE_STACK=yes on a throwaway stack only"

wait_for_stack "$base"
check_headers_and_health "$base"
check_roles 1
scaled=$("${compose[@]}" up -d --no-deps --scale worker=2 worker 2>&1) ||
  fail "a second worker did not start: $scaled"
check_roles 2
# `exec` options, then the service: e.g. --index 2 worker.
unprivileged() {
  local uid
  uid=$("${compose[@]}" exec -T "$@" id -u) || fail "cannot inspect $*"
  [[ $uid != 0 ]] || fail "$* runs as root"
}
unprivileged api
unprivileged --index 1 worker
unprivileged --index 2 worker
unprivileged web
"${compose[@]}" run --rm --no-deps migrate >/dev/null || fail "migrations not re-applied"

demo=$("${compose[@]}" exec -T db psql -U mimorya -d mimorya -tAc \
  "select count(*) from tenants where settings->>'demo' = 'true'") ||
  fail "cannot count the demonstration companies"
[[ $demo == 0 ]] || fail "$demo demonstration companies exist"
page=$(sign_in_page "$base")
if grep -q 'Comptes de démonstration' <<<"$page"; then
  fail "the sign-in page lists demo accounts"
fi

slug=verification-$(date +%s)
output=$("${compose[@]}" exec -T api mimorya-admin create-company \
  --name 'Entreprise de vérification' --slug "$slug" \
  --admin-email "admin@$slug.example" --admin-name 'Administrateur de vérification') ||
  fail "company not created"
link=$(grep -o "$base/invitation#[A-Za-z0-9_-]*" <<<"$output") ||
  fail "no invitation link leading to $base"

password=$(openssl rand -hex 24)
accepted=$(api_post "$base" /auth/invitation/accept \
  "{\"token\":\"${link#*#}\",\"password\":\"$password\"}")
grep -q '^HTTP/[0-9.]* 200' <<<"$accepted" || fail "invitation refused: $(head -1 <<<"$accepted")"
cookie=$(grep -io '^set-cookie: mimorya_session=[^;]*;.*secure' <<<"$accepted" | head -1) ||
  fail "session cookie missing or not secure"
session=${cookie#*mimorya_session=}
session=${session%%;*}

me=$(curl -skf --max-time 10 "$base/api/auth/me" -H "cookie: mimorya_session=$session") ||
  fail "the new session is refused"
grep -q '"name":"Entreprise de vérification"' <<<"$me" || fail "unexpected company: $me"
grep -q '"demo":false' <<<"$me" || fail "the company is marked as a demo: $me"

echo "production test passed: $base"
