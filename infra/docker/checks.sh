# Checks shared by the tests of the containerised stacks (ADR-032, ADR-034),
# sourced by smoke-test.sh and production-test.sh: the stack answers through
# its reverse proxy, with HSTS, the page policy and the API under /api.

fail() {
  echo "$check_name failed: $1" >&2
  exit 1
}

# The stack may still be starting: waits until the proxy reaches both the
# API and the web application.
wait_for_stack() {
  local base=$1
  for _ in $(seq 1 90); do
    if curl -skf --max-time 10 -o /dev/null "$base/api/health" &&
      curl -skf --max-time 10 -o /dev/null "$base/login"; then
      return 0
    fi
    sleep 2
  done
  fail "the stack did not answer through the proxy"
}

check_headers_and_health() {
  local base=$1 page health
  page=$(curl -sk --max-time 10 -D - -o /dev/null "$base/login")
  grep -qi '^strict-transport-security: max-age=63072000' <<<"$page" || fail "no HSTS on the page"
  grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page"
  health=$(curl -skf --max-time 10 "$base/api/health") || fail "API unreachable under /api"
  grep -q '"status":"ok"' <<<"$health" || fail "unexpected health: $health"
}

# Escapes $1 for a JSON string.
json_string() {
  local value=${1//\\/\\\\}
  printf '%s' "${value//\"/\\\"}"
}

# POSTs the JSON $2 to the API path $1, as the browser would (headers and
# status line on stdout, then the body).
api_post() {
  curl -sk --max-time 10 -D - -X POST "$base/api$1" \
    -H 'content-type: application/json' -H 'x-mimorya-request: 1' -H "origin: $base" \
    --data "$2"
}
