#!/usr/bin/env bash
# The production stack behind a reverse proxy already on the server
# (ADR-225), the stack's last test in the CI. Caddy goes; the stack starts
# again with compose.behind-proxy.yml and publishes no port; an nginx stands
# in front, on the mimorya-edge network, with what Nginx Proxy Manager writes
# for a proxy host and the block the guide gives its « Advanced » tab
# (infra/proxy/api-location.conf). Through it, the sign-in page and the API
# under /api, its prefix removed, and a body of 11 MB. The API limits
# sign-ins by the address the proxy appended: a client is limited, a forged
# X-Forwarded-For does not free it, and another client is not limited with
# it. The edge network holds the web application, the API and the proxy
# only: the database is neither named nor reached from it. The proxy
# resolves the names at each request: it follows the API recreated, and it
# starts while the API is stopped, then reaches it once back.
#
# It restarts the web application and the API, removes Caddy and leaves a
# client limited: for a stack thrown away afterwards only (the CI's), hence
# DISPOSABLE_STACK=yes. The stack's compose files come from COMPOSE_FILE,
# without compose.behind-proxy.yml (compose.production.yml by default),
# relative to the repository's root, where it runs from wherever started.
#   DISPOSABLE_STACK=yes infra/docker/behind-proxy-test.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh

check_name="behind-proxy test"
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it restarts the stack: set DISPOSABLE_STACK=yes on a throwaway stack only"
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
nginx_image=nginx:1.27-alpine
curl_image=curlimages/curl:8.11.1
edge=mimorya-edge
front=mimorya-edge-proxy
base=http://127.0.0.1:8080

# Caddy goes: the proxy already on the server holds the ports 80 and 443.
docker compose rm -sf proxy >/dev/null 2>&1 || fail "Caddy was not removed"
docker network create "$edge" >/dev/null || fail "the network $edge was not created"
export COMPOSE_FILE="$COMPOSE_FILE:compose.behind-proxy.yml"
published=$(docker compose config --format json | jq '[.services[] | .ports // [] | length] | add') ||
  fail "the stack's configuration is unreadable"
[[ $published == 0 ]] || fail "the stack publishes $published port(s) behind the proxy"
started=$(docker compose up -d 2>&1) || fail "the stack did not start behind the proxy: $started"
docker compose ps --services | grep -qx proxy && fail "Caddy started behind the proxy"

docker run -d --name "$front" --network "$edge" -p 127.0.0.1:8080:80 \
  -v "$PWD/infra/docker/edge-proxy.conf:/etc/nginx/conf.d/default.conf:ro" \
  -v "$PWD/infra/proxy/api-location.conf:/etc/nginx/mimorya/api-location.conf:ro" \
  "$nginx_image" >/dev/null || fail "the edge proxy did not start"
# The proxy's log when a check fails, then the proxy goes.
trap 'status=$?; ((status == 0)) || docker logs --tail 50 "$front" >&2 || true
      docker rm -f "$front" >/dev/null 2>&1 || true' EXIT
wait_for_stack "$base"

# The status $1 answers through the proxy.
status_of() {
  curl -s --max-time 30 -o /dev/null -w '%{http_code}' "$base$1" ||
    fail "no answer from $1 through the proxy"
}

# The API under /api, its prefix removed once: /health and /auth/options
# are its routes.
health=$(curl -sf --max-time 10 "$base/api/health") || fail "no health under /api"
grep -q '"status":"ok"' <<<"$health" || fail "unexpected health: $health"
options=$(status_of /api/auth/options)
[[ $options == 200 ]] || fail "/api/auth/options answered $options"
page=$(curl -s --max-time 10 -D - -o /dev/null "$base/login") || fail "sign-in page unreachable"
grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page"
# A document's body (10 MB at most) reaches the API, which refuses it
# without a session: neither the proxy's limit (413) nor its default of 1 MB.
deposit=$(head -c $((11 * 1024 * 1024)) /dev/zero |
  curl -s --max-time 60 -o /dev/null -w '%{http_code}' -X POST "$base/api/documents" \
    -H 'content-type: application/octet-stream' -H 'x-mimorya-request: 1' --data-binary @-) ||
  fail "no answer to a deposit of 11 MB through the proxy"
[[ $deposit == 401 || $deposit == 403 ]] || fail "a deposit of 11 MB was answered $deposit, not by the API"

# Only the web application, the API and the proxy are on the edge network.
api=$(docker compose ps --format '{{.Name}}' api)
members=$(docker network inspect -f '{{range .Containers}}{{.Name}} {{end}}' "$edge" | tr ' ' '\n' | sort | xargs)
expected=$(printf '%s\n' "$front" "$api" "$(docker compose ps --format '{{.Name}}' web)" | sort | xargs)
[[ $members == "$expected" ]] || fail "the edge network holds $members, not $expected"

# A sign-in refused, as the host's client (as nginx sees it), with its own
# X-Forwarded-For if given; prints the status.
sign_in_status() {
  local headers=(-H 'content-type: application/json' -H 'x-mimorya-request: 1')
  [[ -n ${1:-} ]] && headers+=(-H "x-forwarded-for: $1")
  curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST "$base/api/auth/login" "${headers[@]}" \
    --data "{\"email\":\"essai-$RANDOM$RANDOM@exemple.invalid\",\"password\":\"pas-le-bon\"}" ||
    fail "no answer to a sign-in through the proxy"
}
# Ten sign-ins a minute by default (LOGIN_RATE_LIMIT_PER_MINUTE), each with
# its own address so that no account's own limit counts: at most one window
# ends while they are sent.
status=
for _ in $(seq 1 22); do
  status=$(sign_in_status)
  [[ $status == 429 ]] && break
done
[[ $status == 429 ]] || fail "the host's client was never limited ($status)"
forged=$(sign_in_status 203.0.113.7)
[[ $forged == 429 ]] || fail "a forged X-Forwarded-For freed the limited client ($forged)"
other=$(docker run --rm --network "$edge" "$curl_image" -s --max-time 10 -o /dev/null \
  -w '%{http_code}' -X POST "http://$front/api/auth/login" \
  -H 'content-type: application/json' -H 'x-mimorya-request: 1' \
  --data '{"email":"autre@exemple.invalid","password":"pas-le-bon"}') ||
  fail "another client got no answer through the proxy"
[[ $other == 401 ]] || fail "another client was limited with the first ($other)"

# From the edge network: curl's exit code reaching $1 (0 when it answered).
reached() {
  local code=0
  docker run --rm --network "$edge" "$curl_image" -s -o /dev/null \
    --connect-timeout 3 --max-time 5 "$1" || code=$?
  ((code < 100)) || fail "the curl container did not run ($code)"
  echo "$code"
}
[[ $(reached http://mimorya-api:4000/health) == 0 ]] || fail "the API is not reached as mimorya-api"
[[ $(reached http://mimorya-web:3000/login) == 0 ]] || fail "the web application is not reached as mimorya-web"
# The IPv4 addresses Docker's resolver gives $1 on the edge network, asked
# from the proxy's container twice: by nslookup of Docker's own resolver,
# then by the C library. A name it does not hold goes to the host's
# resolver, which may answer late, or never: each is cut after 12 s.
answer() {
  docker exec "$front" sh -c \
    'timeout 12 nslookup -type=a "$1" 127.0.0.11; timeout 12 getent ahostsv4 "$1"' sh "$1" 2>&1 || true
}
api_address=$(docker inspect -f "{{(index .NetworkSettings.Networks \"$edge\").IPAddress}}" "$api")
said=$(answer mimorya-api)
grep -qwF "$api_address" <<<"$said" ||
  fail "the resolver does not give mimorya-api's address ($api_address): $said"
db=$(docker compose ps --format '{{.Name}}' db)
address=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$db")
for name in db "$db"; do
  said=$(answer "$name")
  ! grep -qwF "$address" <<<"$said" || fail "the database is named $name on the edge network: $said"
done
code=$(reached "http://$address:5432/")
[[ $code == 7 || $code == 28 ]] || fail "the database is reached at $address from the edge network (curl: $code)"

# Waits until $1 answers $2 through the proxy.
answers() {
  local got=
  for _ in $(seq 1 45); do
    got=$(status_of "$1")
    [[ $got == "$2" ]] && return 0
    sleep 2
  done
  fail "$1 answered $got through the proxy, not $2"
}
# The API recreated, as an update does: the proxy follows it.
recreated=$(docker compose up -d --force-recreate api 2>&1) || fail "the API was not recreated: $recreated"
answers /api/health 200
# The proxy started while the API is stopped, as at a reboot: it starts and
# serves the web application, then reaches the API once back.
docker compose stop api >/dev/null 2>&1 || fail "the API was not stopped"
docker restart "$front" >/dev/null || fail "the proxy did not restart while the API is stopped"
answers /login 200
answers /api/health 502
docker compose start api >/dev/null 2>&1 || fail "the API was not started again"
answers /api/health 200

echo "behind-proxy test passed"
