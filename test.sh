#!/usr/bin/env bash
# Smoke test against a running katago-server.
# Usage: ./test.sh [http://host:port]   (or set KATAGO_SERVER_URL)
#        READY_TIMEOUT=300 ./test.sh    seconds to wait for readiness (default 180)
set -euo pipefail

BASE="${1:-${KATAGO_SERVER_URL:-http://localhost:2718}}"
BASE="${BASE%/}"
case "$BASE" in http://*|https://*) ;; *) BASE="http://${BASE}" ;; esac
READY_TIMEOUT="${READY_TIMEOUT:-180}"

for tool in curl jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is required" >&2; exit 1; }
done

pass=0
fail=0
ok()   { pass=$((pass + 1)); printf '  ok   %s\n' "$*"; }
bad()  { fail=$((fail + 1)); printf '  FAIL %s\n' "$*"; }
check() { # check <description> <shell test...>
  local desc="$1"; shift
  if "$@"; then ok "$desc"; else bad "$desc"; fi
}

echo "Smoke testing ${BASE}"

echo "waiting for /api/v1/health/ready (up to ${READY_TIMEOUT}s)"
elapsed=0
until [ "$(curl -s -o /dev/null -w '%{http_code}' "${BASE}/api/v1/health/ready")" = "200" ]; do
  if [ "$elapsed" -ge "$READY_TIMEOUT" ]; then
    echo "error: server not ready after ${READY_TIMEOUT}s" >&2
    curl -s "${BASE}/api/v1/health" || true
    exit 1
  fi
  sleep 2
  elapsed=$((elapsed + 2))
done
ok "ready after ~${elapsed}s"

echo "GET /api/v1/health"
health="$(curl -sf "${BASE}/api/v1/health")"
check "status is healthy" test "$(jq -r .status <<<"$health")" = "healthy"
check "katago.ready is true" test "$(jq -r .katago.ready <<<"$health")" = "true"

echo "GET /api/v1/version"
version="$(curl -sf "${BASE}/api/v1/version")"
check "server.name" test "$(jq -r .server.name <<<"$version")" = "katago-server"
check "katago.version present" test "$(jq -r '.katago.version // empty' <<<"$version")" != ""
echo "       server $(jq -r .server.version <<<"$version"), katago $(jq -r .katago.version <<<"$version"), model $(jq -r .model.name <<<"$version")"

echo "POST /api/v1/analysis"
analysis="$(curl -sf -X POST "${BASE}/api/v1/analysis" -H 'Content-Type: application/json' \
  -d '{"requestId":"smoke-1","moves":["D4","Q16","R4"],"komi":7.5,"rules":"chinese","boardXSize":19,"boardYSize":19,"maxVisits":10,"includeOwnership":true,"includePolicy":true}')"
check "id echoed" test "$(jq -r .id <<<"$analysis")" = "smoke-1"
check "turnNumber is 3" test "$(jq -r .turnNumber <<<"$analysis")" = "3"
check "moveInfos non-empty" test "$(jq '.moveInfos | length' <<<"$analysis")" -gt 0
check "rootInfo.winrate in [0,1]" jq -e '.rootInfo.winrate >= 0 and .rootInfo.winrate <= 1' <<<"$analysis" >/dev/null
check "ownership has 361 values" test "$(jq '.ownership | length' <<<"$analysis")" = "361"
check "policy has 362 values" test "$(jq '.policy | length' <<<"$analysis")" = "362"
echo "       best move $(jq -r .moveInfos[0].moveCoord <<<"$analysis"), winrate $(jq -r .rootInfo.winrate <<<"$analysis")"

echo "POST /api/v1/analysis (9x9)"
small="$(curl -sf -X POST "${BASE}/api/v1/analysis" -H 'Content-Type: application/json' \
  -d '{"moves":["E5","C3"],"boardXSize":9,"boardYSize":9,"maxVisits":5,"includeOwnership":true}')"
check "ownership has 81 values" test "$(jq '.ownership | length' <<<"$small")" = "81"

echo "POST /api/v1/analysis/game"
game="$(curl -sf -X POST "${BASE}/api/v1/analysis/game" -H 'Content-Type: application/json' \
  -d '{"moves":["D4","Q16","R4"],"maxVisits":5}')"
check "turns count is moves+1 (4)" test "$(jq '.turns | length' <<<"$game")" = "4"
check "turns ordered by turnNumber" test "$(jq -c '[.turns[].turnNumber]' <<<"$game")" = "[0,1,2,3]"

echo "POST /api/v1/analysis (invalid)"
tmp_headers="$(mktemp)"
bad_body="$(curl -s -D "$tmp_headers" -o - -X POST "${BASE}/api/v1/analysis" -H 'Content-Type: application/json' \
  -d '{"moves":["D4","Z99"]}')"
check "status 400" grep -qi '^HTTP/[0-9.]* 400' "$tmp_headers"
check "content-type application/problem+json" grep -qi '^content-type: application/problem+json' "$tmp_headers"
check "problem has field" test "$(jq -r '.field // empty' <<<"$bad_body")" = "moves"
check "problem has type/title/detail" jq -e '.type and .title and .detail' <<<"$bad_body" >/dev/null
rm -f "$tmp_headers"

echo "GET /metrics"
metrics="$(curl -sf "${BASE}/metrics")"
check "contains http_requests_total" grep -q 'http_requests_total' <<<"$metrics"
check "contains katago_engine_up" grep -q 'katago_engine_up' <<<"$metrics"

echo "GET /api/v1/openapi.json"
openapi="$(curl -sf "${BASE}/api/v1/openapi.json")"
check "has /api/v1/analysis path" jq -e '.paths["/api/v1/analysis"]' <<<"$openapi" >/dev/null
check "has /api/v1/analysis/game path" jq -e '.paths["/api/v1/analysis/game"]' <<<"$openapi" >/dev/null

echo "GET /docs"
check "docs served" test "$(curl -s -o /dev/null -w '%{http_code}' "${BASE}/docs")" = "200"

echo
echo "${pass} passed, ${fail} failed"
[ "$fail" -eq 0 ]
