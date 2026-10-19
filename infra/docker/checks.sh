# Checks shared by the tests of the containerised stacks (ADR-032, ADR-034),
# sourced by smoke-test.sh and production-test.sh. The caller names its
# check in `check_name` (failure messages); every function takes the stack's
# base URL first.

fail() {
  echo "${check_name:-check} failed: $1" >&2
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
  page=$(curl -sk --max-time 10 -D - -o /dev/null "$base/login") || fail "sign-in page unreachable"
  grep -qi '^strict-transport-security: max-age=63072000' <<<"$page" || fail "no HSTS on the page"
  grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page"
  health=$(curl -skf --max-time 10 "$base/api/health") || fail "API unreachable under /api"
  grep -q '"status":"ok"' <<<"$health" || fail "unexpected health: $health"
}

# The sign-in page's HTML, as first served.
sign_in_page() {
  curl -skf --max-time 10 "$1/login" || fail "sign-in page unreachable"
}

# Escapes $1 for a JSON string.
json_string() {
  local value=${1//\\/\\\\}
  printf '%s' "${value//\"/\\\"}"
}

# POSTs the JSON $3 to the API path $2 of the stack at $1, as the browser
# would; prints the headers (status line first), then the body.
api_post() {
  curl -sk --max-time 10 -D - -X POST "$1/api$2" \
    -H 'content-type: application/json' -H 'x-mimorya-request: 1' -H "origin: $1" \
    --data "$3" || fail "no answer from $2"
}
