#!/usr/bin/env bash
# Checks the private production stack (ADR-034) through its reverse proxy,
# as an installation starts: HTTPS with HSTS, the page policy and the API
# answer; the API and the web application do not run as root; no
# demonstration account exists; the operator creates a company
# (ADR-033), whose first administrator opens the invitation link, chooses a
# password and gets a secure session in a company that is not a demo.
#   infra/docker/production-test.sh [https://localhost]   (with the stack's env file)
set -euo pipefail
source "$(dirname "$0")/checks.sh"

check_name="production test"
base=${1:-https://localhost}
compose=(docker compose -f compose.production.yml)

wait_for_stack "$base"
check_headers_and_health "$base"
for service in api web; do
  uid=$("${compose[@]}" exec -T "$service" id -u) || fail "cannot inspect $service"
  [[ $uid != 0 ]] || fail "$service runs as root"
done

if curl -sk --max-time 10 "$base/login" | grep -q 'neo-maintenance'; then
  fail "the sign-in page lists demonstration accounts"
fi
demo=$(api_post /auth/login '{"email":"paul.renaud@neo-maintenance.example","password":"demo-mimorya"}')
grep -q '^HTTP/[0-9.]* 401' <<<"$demo" || fail "a demonstration account answered: $(head -1 <<<"$demo")"

slug=verification-$(date +%s)
output=$("${compose[@]}" exec -T api mimorya-admin create-company \
  --name 'Entreprise de vérification' --slug "$slug" \
  --admin-email "admin@$slug.example" --admin-name 'Administrateur de vérification') ||
  fail "company not created"
link=$(grep -o "$base/invitation#[A-Za-z0-9_-]*" <<<"$output") ||
  fail "no invitation link leading to $base"

accepted=$(api_post /auth/invitation/accept \
  "{\"token\":\"${link#*#}\",\"password\":\"phrase-de-passe-de-verification\"}")
grep -q '^HTTP/[0-9.]* 200' <<<"$accepted" || fail "invitation refused: $(head -1 <<<"$accepted")"
cookie=$(grep -io '^set-cookie: mimorya_session=[^;]*;.*secure' <<<"$accepted" | head -1) ||
  fail "session cookie missing or not secure"
session=${cookie#*mimorya_session=}
session=${session%%;*}

me=$(curl -sk --max-time 10 "$base/api/auth/me" -H "cookie: mimorya_session=$session")
grep -q '"name":"Entreprise de vérification"' <<<"$me" || fail "unexpected company: $me"
grep -q '"demo":false' <<<"$me" || fail "the company is marked as a demo: $me"

echo "production test passed: $base"
