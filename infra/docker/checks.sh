# Checks shared by the tests of the containerised stacks (ADR-032, ADR-034),
# sourced by smoke-test.sh and production-test.sh. The caller names its
# check in `check_name` (failure messages); every function takes the stack's
# base URL first, but `check_roles`, which reads the stack's database through
# `docker compose` (COMPOSE_FILE names the stack's files).

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

# Waits until the stack's processes listen as their roles say (ADR-085,
# ADR-087): $1 workers listening for queued runs, on a connection each, and
# one API process relaying run events. An API process that also executed
# runs, or a worker that also served, would count once more. Two readings in
# a row must agree: a process still opening its connections could otherwise
# pass for one in its role.
check_roles() {
  local workers=$1 listening agreed=0
  for _ in $(seq 1 60); do
    listening=$(docker compose exec -T db psql -U mimorya -d mimorya -tAc \
      "select count(*) filter (where application_name = 'mimorya-run-queue') || ' ' ||
              count(*) filter (where application_name = 'mimorya-run-events')
       from pg_stat_activity where datname = 'mimorya'") ||
      fail "cannot read the database's connections"
    if [[ $listening == "$workers 1" ]]; then
      agreed=$((agreed + 1))
      ((agreed == 2)) && return 0
    else
      agreed=0
    fi
    sleep 2
  done
  fail "expected $workers workers and one API process listening (queue, events): $listening"
}
