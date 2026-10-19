#!/usr/bin/env bash
# Checks the containerised demonstration stack through its reverse proxy
# (ADR-032): HTTPS with HSTS, the page policy, the API under /api, and a
# demo account signing in with a secure session cookie.
#   infra/docker/smoke-test.sh [https://localhost]
set -euo pipefail

base=${1:-https://localhost}
fail() {
  echo "smoke test failed: $1" >&2
  exit 1
}

# The stack may still be starting: wait until the proxy reaches both the
# API and the web application.
ready=false
for _ in $(seq 1 90); do
  if curl -skf -o /dev/null "$base/api/health" && curl -skf -o /dev/null "$base/login"; then
    ready=true
    break
  fi
  sleep 2
done
$ready || fail "the stack did not answer through the proxy within 3 minutes"

page=$(curl -sk -D - -o /dev/null "$base/login")
grep -qi '^strict-transport-security: max-age=63072000' <<<"$page" || fail "no HSTS on the page"
grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page"

health=$(curl -skf "$base/api/health") || fail "API unreachable under /api"
grep -q '"status":"ok"' <<<"$health" || fail "unexpected health: $health"

login=$(curl -sk -D - -o /dev/null -X POST "$base/api/auth/login" \
  -H 'content-type: application/json' -H 'x-mimorya-request: 1' -H "origin: $base" \
  --data '{"email":"paul.renaud@neo-maintenance.example","password":"demo-mimorya"}')
grep -q '^HTTP/[0-9.]* 200' <<<"$login" || fail "demo sign-in refused: $(head -1 <<<"$login")"
grep -qi '^set-cookie: mimorya_session=.*secure' <<<"$login" || fail "session cookie not secure"

echo "smoke test passed: $base"
