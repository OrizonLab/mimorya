#!/usr/bin/env bash
# The production stack behind a reverse proxy already on the server
# (ADR-225, ADR-226), the stack's last test in the CI. Caddy goes; the stack
# starts again with compose.behind-proxy.yml and publishes no port. Two
# fronts stand on the mimorya-edge network: an nginx with what Nginx Proxy
# Manager writes for a proxy host and the block the guide gives its
# « Advanced » tab (infra/proxy/api-location.conf), and Nginx Proxy Manager
# itself, that block in its proxy host (proxy-manager.sh). Through each, the
# sign-in page and the API under /api, its prefix removed, and a body of
# 11 MB. The API limits sign-ins by the address the front sees: a client is
# limited, neither a forged X-Forwarded-For nor a forged X-Real-IP frees it,
# by /api/ or by /API/, which the web application relays to the API, and
# another client is not limited with it. The edge network holds the web
# application, the API and the fronts only: the database is neither named
# nor reached from it. The fronts resolve the names at each request: they
# follow the API recreated, and they start while the API is stopped, then
# reach it once back.
#
# It restarts the web application and the API, removes Caddy and leaves
# clients limited: for a stack thrown away afterwards only (the CI's), hence
# DISPOSABLE_STACK=yes. The stack's compose files come from COMPOSE_FILE,
# without compose.behind-proxy.yml (compose.production.yml by default),
# relative to the repository's root, where it runs from wherever started.
#   DISPOSABLE_STACK=yes infra/docker/behind-proxy-test.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
source infra/docker/checks.sh
source infra/docker/proxy-manager.sh

check_name="behind-proxy test"
[[ ${DISPOSABLE_STACK:-} == yes ]] ||
  fail "it restarts the stack: set DISPOSABLE_STACK=yes on a throwaway stack only"
export COMPOSE_FILE=${COMPOSE_FILE:-compose.production.yml}
nginx_image=nginx:1.27-alpine
curl_image=curlimages/curl:8.11.1
edge=mimorya-edge
domain=mimorya.test
nginx_front=mimorya-edge-proxy
manager=mimorya-edge-manager
fronts=("$nginx_front" "$manager")
declare -A port=(["$nginx_front"]=8080 ["$manager"]=8081)
clients=(mimorya-edge-client-1 mimorya-edge-client-2 mimorya-edge-client-3 mimorya-edge-client-4)

# Caddy goes: the proxy already on the server holds the ports 80 and 443.
docker compose rm -sf proxy >/dev/null 2>&1 || fail "Caddy was not removed"
docker network create "$edge" >/dev/null || fail "the network $edge was not created"
export COMPOSE_FILE="$COMPOSE_FILE:compose.behind-proxy.yml"
published=$(docker compose config --format json | jq '[.services[] | .ports // [] | length] | add') ||
  fail "the stack's configuration is unreadable"
[[ $published == 0 ]] || fail "the stack publishes $published port(s) behind the proxy"
started=$(docker compose up -d 2>&1) || fail "the stack did not start behind the proxy: $started"
docker compose ps --services | grep -qx proxy && fail "Caddy started behind the proxy"

# The fronts' logs when a check fails, then the fronts and the clients go.
trap 'status=$?
      if ((status != 0)); then
        for front in "${fronts[@]}"; do docker logs --tail 30 "$front" >&2 || true; done
        docker exec "$manager" sh -c "tail -n 30 /data/logs/proxy-host-*_error.log" >&2 || true
      fi
      docker rm -fv "${fronts[@]}" "${clients[@]}" >/dev/null 2>&1 || true' EXIT
docker run -d --name "$nginx_front" --network "$edge" -p "127.0.0.1:${port[$nginx_front]}:80" \
  -v "$PWD/infra/docker/edge-proxy.conf:/etc/nginx/conf.d/default.conf:ro" \
  -v "$PWD/infra/proxy/api-location.conf:/etc/nginx/mimorya/api-location.conf:ro" \
  "$nginx_image" >/dev/null || fail "the edge proxy did not start"
start_proxy_manager "$manager" "$edge" "${port[$manager]}" 8181 "$domain"

# The status $2 answers through the front $1, 000 when it does not.
status_of() {
  curl -s --max-time 30 -o /dev/null -w '%{http_code}' -H "host: $domain" \
    "http://127.0.0.1:${port[$1]}$2" || true
}
# Waits until $2 answers $3 through the front $1.
answers() {
  local got=
  for _ in $(seq 1 45); do
    got=$(status_of "$1" "$2")
    [[ $got == "$3" ]] && return 0
    sleep 2
  done
  fail "$2 answered $got through $1, not $3"
}

# Through the front $1, the API under /api, its prefix removed once
# (/health and /auth/options are its routes), and the sign-in page. A
# document's body (10 MB at most) reaches the API, which refuses it without
# a session: neither the front's limit (413) nor nginx's default of 1 MB.
through() {
  local front=$1 base=http://127.0.0.1:${port[$1]} health page deposit
  answers "$front" /api/health 200
  answers "$front" /login 200
  health=$(curl -sf --max-time 10 -H "host: $domain" "$base/api/health") ||
    fail "no health under /api through $front"
  grep -q '"status":"ok"' <<<"$health" || fail "unexpected health through $front: $health"
  [[ $(status_of "$front" /api/auth/options) == 200 ]] || fail "/api/auth/options not served through $front"
  page=$(curl -s --max-time 10 -D - -o /dev/null -H "host: $domain" "$base/login") ||
    fail "sign-in page unreachable through $front"
  grep -qi "^content-security-policy: .*'nonce-" <<<"$page" || fail "no nonce policy on the page through $front"
  deposit=$(head -c $((11 * 1024 * 1024)) /dev/zero |
    curl -s --max-time 60 -o /dev/null -w '%{http_code}' -X POST "$base/api/documents" -H "host: $domain" \
      -H 'content-type: application/octet-stream' -H 'x-mimorya-request: 1' --data-binary @-) ||
    fail "no answer to a deposit of 11 MB through $front"
  [[ $deposit == 401 || $deposit == 403 ]] ||
    fail "a deposit of 11 MB through $front was answered $deposit, not by the API"
}
for front in "${fronts[@]}"; do through "$front"; done

# Only the web application, the API and the fronts are on the edge network.
api=$(docker compose ps --format '{{.Name}}' api)
members=$(docker network inspect -f '{{range .Containers}}{{.Name}} {{end}}' "$edge" | tr ' ' '\n' | sort | xargs)
expected=$(printf '%s\n' "${fronts[@]}" "$api" "$(docker compose ps --format '{{.Name}}' web)" | sort | xargs)
[[ $members == "$expected" ]] || fail "the edge network holds $members, not $expected"

# Two clients for each front, on the edge network until the end, so that no
# client's address is given again to another.
for client in "${clients[@]}"; do
  docker run -d --name "$client" --network "$edge" --entrypoint sleep "$curl_image" 900 >/dev/null ||
    fail "the client $client did not start"
done
# A sign-in refused, from the client $1 through the front $2 at the path $3,
# with the header $4 if given; prints the status.
sign_in_status() {
  local headers=(-H "host: $domain" -H 'content-type: application/json' -H 'x-mimorya-request: 1')
  [[ -n ${4:-} ]] && headers+=(-H "$4")
  docker exec "$1" curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST \
    "http://$2$3" "${headers[@]}" \
    --data "{\"email\":\"essai-$RANDOM$RANDOM@exemple.invalid\",\"password\":\"pas-le-bon\"}" ||
    fail "no answer to a sign-in from $1 through $2"
}
# Through the front $1, the client $2 is limited and the client $3 is not.
# Ten sign-ins a minute by default (LOGIN_RATE_LIMIT_PER_MINUTE), each with
# its own e-mail address so that no account's own limit counts: at most one
# window ends while they are sent. Then no header a client may send frees
# it, by /api/ or by /API/, which the front serves as « location / » and the
# web application relays to the API: Nginx Proxy Manager takes X-Real-IP
# for $remote_addr from any private address, its own included, unless the
# block's server-wide line stops it.
limits() {
  local front=$1 limited=$2 other=$3 status= path forged
  for _ in $(seq 1 22); do
    status=$(sign_in_status "$limited" "$front" /api/auth/login)
    [[ $status == 429 ]] && break
  done
  [[ $status == 429 ]] || fail "$limited was never limited through $front ($status)"
  for path in /api/auth/login /API/auth/login; do
    # The path reaches the API, which refuses the other client's sign-in.
    status=$(sign_in_status "$other" "$front" "$path")
    [[ $status == 401 ]] || fail "another client was answered $status at $path through $front, not 401"
    for forged in '' 'x-forwarded-for: 203.0.113.7' 'x-real-ip: 203.0.113.8'; do
      status=$(sign_in_status "$limited" "$front" "$path" "$forged")
      [[ $status == 429 ]] ||
        fail "the limited client was answered $status at $path through $front${forged:+, with a forged ${forged%%:*}}"
    done
  done
}
limits "$nginx_front" "${clients[0]}" "${clients[1]}"
limits "$manager" "${clients[2]}" "${clients[3]}"

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
# from the nginx front's container twice: by nslookup of Docker's own
# resolver, then by the C library. A name it does not hold goes to the
# host's resolver, which may answer late, or never: each is cut after 12 s.
answer() {
  docker exec "$nginx_front" sh -c \
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

# The API recreated, as an update does: each front follows it.
recreated=$(docker compose up -d --force-recreate api 2>&1) || fail "the API was not recreated: $recreated"
for front in "${fronts[@]}"; do answers "$front" /api/health 200; done
# Each front started while the API is stopped, as at a reboot: it starts
# and serves the web application, then reaches the API once back.
docker compose stop api >/dev/null 2>&1 || fail "the API was not stopped"
for front in "${fronts[@]}"; do
  docker restart "$front" >/dev/null || fail "$front did not restart while the API is stopped"
  answers "$front" /login 200
  answers "$front" /api/health 502
done
docker compose start api >/dev/null 2>&1 || fail "the API was not started again"
for front in "${fronts[@]}"; do answers "$front" /api/health 200; done

echo "behind-proxy test passed"
