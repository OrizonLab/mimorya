# Checks shared by the tests of the containerised stacks (ADR-032, ADR-034),
# sourced by smoke-test.sh, production-test.sh and backup-test.sh (ADR-166).
# The caller names its check in `check_name` (failure messages); every
# function takes the stack's base URL first, but `session_of`,
# `stack_workers` and `check_roles`. These two read the stack through
# `docker compose` (COMPOSE_FILE names its files) and jq; `open_company`
# reaches it through `docker compose` too.

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

# Prints the session of the Set-Cookie headers in $1, checked secure.
session_of() {
  local cookie session
  cookie=$(grep -io '^set-cookie: mimorya_session=[^;]*;.*secure' <<<"$1" | head -1) ||
    fail "session cookie missing or not secure"
  session=${cookie#*mimorya_session=}
  printf '%s' "${session%%;*}"
}

# The operator creates the company named $3, its slug $2 (ADR-033); its
# first administrator, admin@$2.example, opens the invitation link, which
# leads to the stack at $1, and chooses the password $4. Prints the session
# they get.
open_company() {
  local base=$1 slug=$2 name=$3 password=$4 output link accepted
  output=$(docker compose exec -T api mimorya-admin create-company \
    --name "$name" --slug "$slug" \
    --admin-email "admin@$slug.example" --admin-name 'Administrateur de vérification') ||
    fail "company not created"
  link=$(grep -o "$base/invitation#[A-Za-z0-9_-]*" <<<"$output") ||
    fail "no invitation link leading to $base"
  accepted=$(api_post "$base" /auth/invitation/accept \
    "{\"token\":\"${link#*#}\",\"password\":\"$password\"}")
  grep -q '^HTTP/[0-9.]* 200' <<<"$accepted" || fail "invitation refused: $(head -1 <<<"$accepted")"
  session_of "$accepted"
}

# Signs in as $2 with the password $3; prints the session.
sign_in() {
  local base=$1 email=$2 password=$3 answer
  answer=$(api_post "$base" /auth/login \
    "{\"email\":\"$(json_string "$email")\",\"password\":\"$(json_string "$password")\"}")
  grep -q '^HTTP/[0-9.]* 200' <<<"$answer" || fail "sign-in refused: $(head -1 <<<"$answer")"
  session_of "$answer"
}

# The session $2 is accepted, in the company named $3, which is not a demo.
check_company_of() {
  local base=$1 session=$2 name=$3 me
  me=$(curl -skf --max-time 10 "$base/api/auth/me" -H "cookie: mimorya_session=$session") ||
    fail "the session is refused"
  grep -q "\"name\":\"$name\"" <<<"$me" || fail "unexpected company: $me"
  grep -q '"demo":false' <<<"$me" || fail "the company is marked as a demo: $me"
}

# How many workers the stack runs (MIMORYA_WORKERS, 1 by default; ADR-087).
stack_workers() {
  docker compose config --format json | jq -er '.services.worker.scale // 1' ||
    fail "cannot read the stack's worker count (docker compose config, jq)"
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
