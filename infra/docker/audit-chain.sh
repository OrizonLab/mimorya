# The audit chain of a stack's company as its operator and its administrator
# see it (ADR-178, ADR-179, ADR-180), sourced by backup-test.sh after
# checks.sh. The stack runs with a master key: its workers seal each
# company's new entries every minute and log the anchor of the last place
# sealed, `anchor=<company>:<place>:<seal>`. Reads the database as its owner
# and the logs through `docker compose` (COMPOSE_FILE names its files), the
# administrator's report with jq.

# The identifier of the company of slug $1.
company_id() {
  docker compose exec -T db psql -U mimorya -d mimorya -tAc \
    "select id from tenants where slug = '$1'" || fail "cannot read the company $1"
}

# The anchor of the furthest place the stack's processes logged for the
# company $1, empty if none: sealed by a pass (`Audit log sealed`) or found
# by a process at its start (`Audit chain found`), the same seal at the
# same place. Places only grow until a restore, after which the logs of the
# processes before it are gone with them.
logged_anchor() {
  docker compose logs --no-color 2>/dev/null |
    grep -o "anchor=$1:[0-9]*:[0-9a-f]\{64\}" |
    cut -d= -f2 | sort -t: -k2,2n | tail -1
}

# Waits until every entry of the company $1 is sealed and the sealing logged
# its last place; prints that anchor, as the operator keeps it.
sealed_anchor() {
  local tenant=$1 state='' unsealed head anchor=''
  for _ in $(seq 1 60); do
    state=$(docker compose exec -T db psql -U mimorya -d mimorya -tAc \
      "select count(*) filter (where chain_seq is null) || ' ' || coalesce(max(chain_seq), 0)
         from audit_log where tenant_id = '$tenant'") || fail "cannot read the audit log"
    unsealed=${state% *} head=${state#* }
    anchor=$(logged_anchor "$tenant")
    if ((unsealed == 0 && head > 0)) && [[ ${anchor#*:} == "$head":* ]]; then
      printf '%s' "$anchor"
      return 0
    fi
    sleep 3
  done
  fail "the audit log of $tenant was not sealed (unsealed, last place: $state; last anchor logged: ${anchor:-none})"
}

# The operator verifies every company's chain against the anchors $@
# (mimorya-admin verify-audit); prints what it says, fails as it does.
operator_verifies() {
  local args=() anchor
  for anchor in "$@"; do args+=(--anchor "$anchor"); done
  docker compose exec -T api mimorya-admin verify-audit "${args[@]}" 2>&1
}

# The administrator of session $2 verifies their company's chain at the
# stack at $1, against the anchors $3… (ADR-179); prints the report.
company_verifies() {
  local base=$1 session=$2 anchors='' anchor answer
  shift 2
  for anchor in "$@"; do anchors+="${anchors:+,}\"$anchor\""; done
  answer=$(curl -sk --max-time 60 -X POST "$base/api/admin/audit/verifications" \
    -H 'content-type: application/json' -H 'x-mimorya-request: 1' -H "origin: $base" \
    -H "cookie: mimorya_session=$session" --data "{\"anchors\":[$anchors]}") ||
    fail "no answer from the verification of the audit log"
  jq -e '.holds | type == "boolean"' <<<"$answer" >/dev/null ||
    fail "the verification of the audit log answered: $answer"
  printf '%s' "$answer"
}

# The report $1 holds or not ($2: true or false), each anchor found as the
# extended regular expression $3 says (held, differs or missing, joined by
# commas).
check_report() {
  local report=$1 holds=$2 found=$3 read
  read=$(jq -r '"\(.holds) \([.anchors[].found] | join(","))"' <<<"$report")
  [[ $read =~ ^$holds\ $found$ ]] ||
    fail "the company's verification found '$read', not '$holds $found': $report"
}
