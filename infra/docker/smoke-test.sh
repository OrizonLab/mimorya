#!/usr/bin/env bash
# Checks the containerised demonstration stack through its reverse proxy
# (ADR-032): HTTPS with HSTS, the page policy, the API under /api, the
# sign-in page listing the demo accounts, and one of them signing in, with
# the stack's DEMO_PASSWORD, and getting a secure session cookie; and, in
# its database, one API process and one worker, each in its role (ADR-087).
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
check_roles 1
page=$(sign_in_page "$base")
grep -q 'Comptes de démonstration' <<<"$page" || fail "the sign-in page lists no demo account"

login=$(api_post "$base" /auth/login \
  "{\"email\":\"paul.renaud@neo-maintenance.example\",\"password\":\"$password\"}")
grep -q '^HTTP/[0-9.]* 200' <<<"$login" || fail "demo sign-in refused: $(head -1 <<<"$login")"
grep -qi '^set-cookie: mimorya_session=.*secure' <<<"$login" || fail "session cookie not secure"

echo "smoke test passed: $base"
