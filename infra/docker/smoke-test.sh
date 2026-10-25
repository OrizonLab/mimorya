#!/usr/bin/env bash
# Checks the containerised demonstration stack through its reverse proxy
# (ADR-032): HTTPS with HSTS, the page policy, the API under /api, the
# sign-in page listing the demo accounts, and one of them signing in, with
# the stack's DEMO_PASSWORD, and getting a secure session cookie; and, in
# its database, one API process and the stack's workers (MIMORYA_WORKERS),
# each in its role (ADR-087).
# The stack's compose files come from COMPOSE_FILE (compose.demo.yml by
# default; the CI adds compose.demo.build.yml, which builds the images),
# relative to the repository's root, where it runs from wherever started.
#   DEMO_PASSWORD=… infra/docker/smoke-test.sh [https://localhost]
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh

check_name="smoke test"
base=${1:-https://localhost}
password=$(json_string "${DEMO_PASSWORD:?DEMO_PASSWORD of the stack is required}")
export COMPOSE_FILE=${COMPOSE_FILE:-compose.demo.yml}

wait_for_stack "$base"
check_headers_and_health "$base"
workers=$(stack_workers)
check_roles "$workers"
page=$(sign_in_page "$base")
grep -q 'Comptes de démonstration' <<<"$page" || fail "the sign-in page lists no demo account"

login=$(api_post "$base" /auth/login \
  "{\"email\":\"paul.renaud@neo-maintenance.example\",\"password\":\"$password\"}")
grep -q '^HTTP/[0-9.]* 200' <<<"$login" || fail "demo sign-in refused: $(head -1 <<<"$login")"
grep -qi '^set-cookie: mimorya_session=.*secure' <<<"$login" || fail "session cookie not secure"

# A document deposited through the web relay is read by the extraction
# process the image ships beside the API (ADR-095), then deleted.
login=$(api_post "$base" /auth/login \
  "{\"email\":\"claire.martin@neo-maintenance.example\",\"password\":\"$password\"}")
cookie=$(grep -io '^set-cookie: mimorya_session=[^;]*' <<<"$login" | head -1) ||
  fail "director sign-in refused: $(head -1 <<<"$login")"
session=${cookie#*mimorya_session=}
deposit=$(curl -sk --max-time 30 -D - -X POST "$base/api/documents?name=Verification.md" \
  -H 'content-type: text/markdown' -H 'x-mimorya-request: 1' -H "origin: $base" \
  -H "cookie: mimorya_session=$session" \
  --data-binary $'# Vérification\n\nDocument déposé par le test de fumée.') ||
  fail "no answer to the deposit"
grep -q '^HTTP/[0-9.]* 201' <<<"$deposit" || fail "document refused: $(tail -1 <<<"$deposit")"
grep -q '"passages":1' <<<"$deposit" || fail "document not cut into a passage: $(tail -1 <<<"$deposit")"
document=$(grep -o '"id":"[0-9a-f-]*"' <<<"$deposit" | head -1 | cut -d'"' -f4)
deleted=$(curl -sk --max-time 10 -o /dev/null -w '%{http_code}' -X DELETE \
  "$base/api/documents/$document" -H 'x-mimorya-request: 1' -H "origin: $base" \
  -H "cookie: mimorya_session=$session")
[ "$deleted" = 200 ] || fail "document not deleted: $deleted"

echo "smoke test passed: $base"
