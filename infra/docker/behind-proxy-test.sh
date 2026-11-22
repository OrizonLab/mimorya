#!/usr/bin/env bash
# The production stack behind a reverse proxy already on the server
# (ADR-225), the stack's last test in the CI. Caddy goes; the stack starts
# again with compose.behind-proxy.yml and publishes no port; an nginx stands
# in front, on the mimorya-edge network, with what Nginx Proxy Manager writes
# for a proxy host and the block the guide gives its « Advanced » tab
# (infra/proxy/api-location.conf). Through it, the sign-in page and the API
# under /api, its prefix removed. The API limits sign-ins by the address the
# proxy appended: a client is limited, a forged X-Forwarded-For does not free
# it, and another client is not limited with it. The edge network holds the
# web application, the API and the proxy only: the database is neither
# named nor reached from it.
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
published=$(docker compose config --format json | jq '[.services[] | .ports // [] | length] | add')
[[ $published == 0 ]] || fail "the stack publishes $published port(s) behind the proxy"
started=$(docker compose up -d 2>&1) || fail "the stack did not start behind the proxy: $started"
docker compose ps --services | grep -qx proxy && fail "Caddy started behind the proxy"

docker run -d --name "$front" --network "$edge" -p 127.0.0.1:8080:80 \
  -v "$PWD/infra/docker/edge-proxy.conf:/etc/nginx/conf.d/default.conf:ro" \
  -v "$PWD/infra/proxy/api-location.conf:/etc/nginx/mimorya/api-location.conf:ro" \
  "$nginx_image" >/dev/null || fail "the edge proxy did not start"
trap 'docker rm -f "$front" >/dev/null 2>&1 || true' EXIT
wait_for_stack "$base"

# The API under /api, its prefix removed once: /health and /auth/options
# are its routes.
health=$(curl -sf --max-time 10 "$base/api/health") || fail "no health under /api"
grep -q '"status":"ok"' <<<"$health" || fail "unexpected health: $health"
options=$(curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$base/api/auth/options")
[[ $options == 200 ]] || fail "/api/auth/options answered $options"
page=$(curl -s --max-time 10 -D - -o /dev/null "$base/login") || fail "sign-in page unreachable"
grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page"

# Only the web application, the API and the proxy are on the edge network.
members=$(docker network inspect -f '{{range .Containers}}{{.Name}} {{end}}' "$edge" | tr ' ' '\n' | sort | xargs)
expected=$(printf '%s\n' "$front" "$(docker compose ps --format '{{.Name}}' api)" \
  "$(docker compose ps --format '{{.Name}}' web)" | sort | xargs)
[[ $members == "$expected" ]] || fail "the edge network holds $members, not $expected"

# A sign-in refused, as the host's client (as nginx sees it), with its own
# X-Forwarded-For if given; prints the status.
sign_in_status() {
  local headers=(-H 'content-type: application/json' -H 'x-mimorya-request: 1')
  [[ -n ${1:-} ]] && headers+=(-H "x-forwarded-for: $1")
  curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST "$base/api/auth/login" "${headers[@]}" \
    --data "{\"email\":\"essai-$RANDOM$RANDOM@exemple.invalid\",\"password\":\"pas-le-bon\"}"
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
  --data '{"email":"autre@exemple.invalid","password":"pas-le-bon"}')
[[ $other == 401 ]] || fail "another client was limited with the first ($other)"

# From the edge network: curl's exit code reaching $1 (0 when it answered).
reached() {
  local code=0
  docker run --rm --network "$edge" "$curl_image" -s -o /dev/null \
    --connect-timeout 3 --max-time 5 "$1" || code=$?
  echo "$code"
}
[[ $(reached http://mimorya-api:4000/health) == 0 ]] || fail "the API is not reached as mimorya-api"
[[ $(reached http://mimorya-web:3000/login) == 0 ]] || fail "the web application is not reached as mimorya-web"
db=$(docker compose ps --format '{{.Name}}' db)
for name in db "$db"; do
  code=$(reached "http://$name:5432/")
  [[ $code == 6 ]] || fail "the database is named $name on the edge network (curl: $code)"
done
address=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$db")
code=$(reached "http://$address:5432/")
[[ $code == 7 || $code == 28 ]] || fail "the database is reached at $address from the edge network (curl: $code)"

echo "behind-proxy test passed"
