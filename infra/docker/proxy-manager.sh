# Nginx Proxy Manager itself in front of the stack (ADR-226), sourced by
# behind-proxy-test.sh after checks.sh: its proxy host is declared through
# its API as the guide has its form filled (docs/INSTALLATION.md, annex):
# the platform's domain forwarded to mimorya-web:3000, Block Common Exploits
# off, the block of infra/proxy/api-location.conf in its « Advanced » tab.
# Without a certificate: Let's Encrypt cannot reach the CI. Its address
# ranges are not fetched (CloudFront, Cloudflare), nor is IPv6 listened on:
# the runner's containers have none. Its two folders are volumes, as it
# requires, removed with it (docker rm -v).
proxy_manager_image=jc21/nginx-proxy-manager:2.16.0

# Starts the Proxy Manager named $1 on the network $2, serving on the host's
# 127.0.0.1:$3 and administered on 127.0.0.1:$4, then declares the proxy
# host of the domain $5. Fails unless its nginx took the host's
# configuration.
start_proxy_manager() {
  local name=$1 network=$2 port=$3 admin=$4 domain=$5
  local email=essai@exemple.invalid password=essai-$RANDOM$RANDOM token= host id meta
  docker run -d --name "$name" --network "$network" -v /data -v /etc/letsencrypt \
    -p "127.0.0.1:$port:80" -p "127.0.0.1:$admin:81" \
    -e DISABLE_IPV6=true -e IP_RANGES_FETCH_ENABLED=false \
    -e INITIAL_ADMIN_EMAIL="$email" -e INITIAL_ADMIN_PASSWORD="$password" \
    "$proxy_manager_image" >/dev/null || fail "Nginx Proxy Manager did not start"
  local api=http://127.0.0.1:$admin/api
  for _ in $(seq 1 90); do
    [[ $(docker inspect -f '{{.State.Running}}' "$name") == true ]] ||
      fail "Nginx Proxy Manager stopped as it started"
    token=$(curl -sf --max-time 5 "$api/tokens" -H 'content-type: application/json' \
      --data "{\"identity\":\"$email\",\"secret\":\"$password\"}" | jq -r '.token // empty') || true
    [[ -n $token ]] && break
    sleep 2
  done
  [[ -n $token ]] || fail "Nginx Proxy Manager gave no token to its administrator"
  host=$(jq -n --arg domain "$domain" --rawfile block infra/proxy/api-location.conf '{
      domain_names: [$domain], forward_scheme: "http", forward_host: "mimorya-web",
      forward_port: 3000, block_exploits: false, allow_websocket_upgrade: false,
      caching_enabled: false, advanced_config: $block}' |
    curl -s --max-time 60 "$api/nginx/proxy-hosts" -H "authorization: Bearer $token" \
      -H 'content-type: application/json' --data @-) ||
    fail "Nginx Proxy Manager did not answer the proxy host's declaration"
  id=$(jq -r '.id // empty' <<<"$host")
  [[ -n $id ]] || fail "Nginx Proxy Manager refused the proxy host: $host"
  meta=$(curl -sf --max-time 10 "$api/nginx/proxy-hosts/$id" -H "authorization: Bearer $token" |
    jq -c '.meta') || fail "the proxy host $id is unreadable"
  [[ $(jq -r '.nginx_online' <<<"$meta") == true ]] ||
    fail "Nginx Proxy Manager's nginx did not take the proxy host: $meta"
}
