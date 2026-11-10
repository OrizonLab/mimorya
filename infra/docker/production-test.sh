#!/usr/bin/env bash
# Checks a freshly installed production stack (ADR-034) through its reverse
# proxy, as an installation starts: HTTPS with HSTS, the page policy and the
# API answer; one API process and the stack's workers (MIMORYA_WORKERS),
# each in its role, then one worker more side by side (ADR-086, ADR-087);
# the API, every worker and the web application do not run as root; the migrations apply again without
# change; no demonstration company exists and the sign-in page lists no demo
# account; the operator creates a company (ADR-033), whose first
# administrator opens the invitation link, chooses a password and gets a
# secure session in a company that is not a demo; the operator suspends the
# company, whose session is refused at once, then reactivates it, whose
# session is accepted again (ADR-191); suspended again, the company is shown,
# then deleted as the database's owner through the `migrate` service, its
# administrator no longer let in and nothing of it left (ADR-192).
#
# It creates and deletes a company: for a stack thrown away afterwards only
# (the CI's), hence DISPOSABLE_STACK=yes. The
# stack's variables come from the environment (or COMPOSE_ENV_FILES), its
# compose files from COMPOSE_FILE (compose.production.yml by default; the CI
# adds compose.production.build.yml, which builds the images), relative to
# the repository's root, where it runs from wherever started.
#   DISPOSABLE_STACK=yes infra/docker/production-test.sh [https://localhost]
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh

check_name="production test"
base=${1:-https://localhost}
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
compose=(docker compose)
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it creates and deletes a company: set DISPOSABLE_STACK=yes on a throwaway stack only"

wait_for_stack "$base"
check_headers_and_health "$base"
workers=$(stack_workers)
check_roles "$workers"
more=$((workers + 1))
scaled=$("${compose[@]}" up -d --no-deps --scale "worker=$more" worker 2>&1) ||
  fail "one worker more did not start: $scaled"
check_roles "$more"
# `exec` options, then the service: e.g. --index 2 worker.
unprivileged() {
  local uid
  uid=$("${compose[@]}" exec -T "$@" id -u) || fail "cannot inspect $*"
  [[ $uid != 0 ]] || fail "$* runs as root"
}
unprivileged api
for index in $(seq 1 "$more"); do
  unprivileged --index "$index" worker
done
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
session=$(open_company "$base" "$slug" 'Entreprise de vérification' "$(openssl rand -hex 24)")
check_company_of "$base" "$session" 'Entreprise de vérification'

suspended=$("${compose[@]}" exec -T api mimorya-admin suspend-company --slug "$slug") ||
  fail "company not suspended: $suspended"
refused=$(curl -sk -o /dev/null -w '%{http_code}' --max-time 10 "$base/api/auth/me" \
  -H "cookie: mimorya_session=$session")
[[ $refused == 401 ]] || fail "the suspended company's session answered $refused, not 401"
reactivated=$("${compose[@]}" exec -T api mimorya-admin reactivate-company --slug "$slug") ||
  fail "company not reactivated: $reactivated"
check_company_of "$base" "$session" 'Entreprise de vérification'

# The company deleted (ADR-192): suspended first, shown without its slug
# confirmed (nothing deleted), then deleted as the database's owner, the
# only role that may, through the service that holds its address.
"${compose[@]}" exec -T api mimorya-admin suspend-company --slug "$slug" >/dev/null ||
  fail "company not suspended before its deletion"
shown=$("${compose[@]}" run --rm --no-deps migrate mimorya-admin delete-company --slug "$slug") ||
  fail "company not shown: $shown"
grep -q 'Rien n’a été supprimé' <<<"$shown" || fail "deleted without its slug confirmed: $shown"
deleted=$("${compose[@]}" run --rm --no-deps migrate \
  mimorya-admin delete-company --slug "$slug" --confirm "$slug") ||
  fail "company not deleted: $deleted"
left=$("${compose[@]}" exec -T db psql -U mimorya -d mimorya -tAc \
  "select count(*) from tenants where slug = '$slug'") || fail "cannot count the companies"
[[ $left == 0 ]] || fail "the deleted company is still there"
refused=$(curl -sk -o /dev/null -w '%{http_code}' --max-time 10 "$base/api/auth/me" \
  -H "cookie: mimorya_session=$session")
[[ $refused == 401 ]] || fail "the deleted company's session answered $refused, not 401"

echo "production test passed: $base"
