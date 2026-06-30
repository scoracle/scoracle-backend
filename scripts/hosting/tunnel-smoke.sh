#!/usr/bin/env bash
# Tunnel / API smoke test.
#
# Exercises the endpoints that actually matter for a frontend connection:
# process health, sport health, per-product entity reads, leaderboards, and
# CORS preflight. Historical live integration routes are intentionally not probed.
#
# Run after any of these changes:
#   - Cloudflare Tunnel config edited
#   - CORS_* env vars adjusted
#   - ENVIRONMENT flipped between development / production
#   - API binary rebuilt + restarted
#   - New DNS record pointed at the tunnel
#
# Usage:
#   scripts/hosting/tunnel-smoke.sh                              # local API
#   scripts/hosting/tunnel-smoke.sh https://api.<YOUR-DOMAIN>    # via CF Tunnel
#   scripts/hosting/tunnel-smoke.sh https://api.<YOUR-DOMAIN> https://app.<YOUR-DOMAIN>
#   scripts/hosting/tunnel-smoke.sh --full https://api.<YOUR-DOMAIN>  # accepted for old runbooks; no extra checks
#
# Replace <YOUR-DOMAIN> with a domain YOU OWN and have tunneled — not a
# placeholder. Running against an unowned domain will get generic CF
# edge responses (200 OK with ~1KB HTML bodies) and eventually 429s.
#
# Exit 0 if every check passes. Exit 1 if any fails. Prints a table of
# results so a human can spot which specific endpoint regressed.

set -uo pipefail

# ---------------------------------------------------------------------------
# Arg parsing
# ---------------------------------------------------------------------------

FULL=0
if [[ "${1:-}" == "--full" ]]; then
    FULL=1
    shift
fi

HOST=${1:-http://localhost:8000}
HOST=${HOST%/}   # trim trailing slash

# Default origin tracks the host: local runs want the Astro dev origin,
# tunnel runs want the real frontend origin. Override with arg 2.
if [[ "$HOST" == *localhost* || "$HOST" == *127.0.0.1* ]]; then
    DEFAULT_ORIGIN="http://localhost:4321"
else
    DEFAULT_ORIGIN="https://scoracle.com"
fi
ORIGIN=${2:-$DEFAULT_ORIGIN}

# Known test entities — adjust if your DB doesn't have these. NBA 115 is
# Stephen Curry; 14 is the Lakers. Both likely have data.
PLAYER_ID=${SCORACLE_TEST_PLAYER_ID:-115}
TEAM_ID=${SCORACLE_TEST_TEAM_ID:-14}

# ---------------------------------------------------------------------------
# Pretty-printing helpers
# ---------------------------------------------------------------------------

GREEN=$'\e[32m'; RED=$'\e[31m'; YELLOW=$'\e[33m'; DIM=$'\e[2m'; RESET=$'\e[0m'
# Disable colors when not writing to a terminal.
if [[ ! -t 1 ]]; then GREEN=""; RED=""; YELLOW=""; DIM=""; RESET=""; fi

FAILS=0
PASSES=0
WARNS=0

emit() {
    local kind=$1 label=$2 detail=${3:-}
    case "$kind" in
        pass)  printf '  %s%-8s%s %-42s %s\n' "$GREEN" "PASS" "$RESET" "$label" "$detail" ; PASSES=$((PASSES + 1)) ;;
        fail)  printf '  %s%-8s%s %-42s %s\n' "$RED"   "FAIL" "$RESET" "$label" "$detail" ; FAILS=$((FAILS + 1)) ;;
        warn)  printf '  %s%-8s%s %-42s %s\n' "$YELLOW" "WARN" "$RESET" "$label" "$detail" ; WARNS=$((WARNS + 1)) ;;
        skip)  printf '  %s%-8s%s %-42s %s\n' "$DIM"   "SKIP" "$RESET" "$label" "$detail" ;;
    esac
}

# ---------------------------------------------------------------------------
# Curl wrappers
# ---------------------------------------------------------------------------

# Returns: HTTP status code + total time (seconds, 3dp) + body bytes
probe() {
    local url=$1
    curl -sS -o /tmp/smoke-body.$$ \
         -w '%{http_code} %{time_total}s %{size_download}B' \
         --max-time 15 \
         "$url" 2>/tmp/smoke-err.$$ || echo "000 err -"
}

# CORS preflight — OPTIONS with Origin + Access-Control-Request-Method.
# Deliberately omit Access-Control-Request-Headers: browsers only send it
# when the actual request carries non-safelisted headers (Content-Type
# other than the three CORS-safelisted values, custom auth headers, etc).
# A plain GET with Accept doesn't trigger that branch.
probe_cors() {
    local url=$1 origin=$2
    curl -sSI -X OPTIONS \
         -H "Origin: $origin" \
         -H "Access-Control-Request-Method: GET" \
         --max-time 10 \
         "$url" 2>/dev/null
}

# ---------------------------------------------------------------------------
# Individual checks
# ---------------------------------------------------------------------------

echo "Smoke test against $HOST"
echo "CORS origin under test: $ORIGIN"
echo

check_200() {
    local label=$1 url=$2
    local result status time_s body_size
    result=$(probe "$url")
    status=${result%% *}
    result=${result#* }
    time_s=${result%% *}
    body_size=${result#* }
    if [[ "$status" == "200" ]]; then
        emit pass "$label" "$status  ${time_s}  ${body_size}"
    else
        emit fail "$label" "got $status  $(cat /tmp/smoke-err.$$ 2>/dev/null | head -c 80)"
    fi
    rm -f /tmp/smoke-body.$$ /tmp/smoke-err.$$
}

check_200_or_404() {
    # For generated entity products, 404 can be a legitimate "not generated
    # yet" answer, not an infra failure. Distinguish in output.
    local label=$1 url=$2
    local result status
    result=$(probe "$url")
    status=${result%% *}
    case "$status" in
        200) emit pass "$label" "$status (blurb present)" ;;
        404) emit warn "$label" "$status (no blurb yet — expected before first generation)" ;;
        *)   emit fail "$label" "got $status" ;;
    esac
    rm -f /tmp/smoke-body.$$ /tmp/smoke-err.$$
}

check_cors() {
    local label=$1 url=$2 origin=$3
    local headers allow_origin
    headers=$(probe_cors "$url" "$origin")
    allow_origin=$(printf '%s' "$headers" | grep -i '^access-control-allow-origin:' | tr -d '\r\n')
    if [[ -z "$allow_origin" ]]; then
        emit fail "$label" "no Access-Control-Allow-Origin header for $origin"
        return
    fi
    if [[ "$allow_origin" == *"$origin"* ]] || [[ "$allow_origin" == *"*"* ]]; then
        emit pass "$label" "$allow_origin"
    else
        emit fail "$label" "got '$allow_origin' (origin $origin not whitelisted)"
    fi
}

# Origin-less probe: how a server-to-server caller (Cloudflare worker doing
# SSR fetches via "use server", a uptime monitor, etc.) actually hits the API.
# A 403 here means an upstream WAF/Bot-Fight rule on the api.scoracle.com zone
# is rejecting non-browser traffic; the Go server itself emits no 403s.
check_origin_less() {
    local label=$1 url=$2
    local result status
    result=$(curl -sS -o /dev/null \
        -w '%{http_code}' \
        --max-time 15 \
        -H 'User-Agent: scoracle-smoke/1.0' \
        "$url" 2>/dev/null || echo "000")
    status=$result
    case "$status" in
        200) emit pass "$label" "$status (origin-less GET succeeded)" ;;
        403) emit fail "$label" "$status — likely WAF/Bot-Fight on the api zone (no Origin header)" ;;
        *)   emit fail "$label" "got $status" ;;
    esac
}

# ---------------------------------------------------------------------------
# Run checks
# ---------------------------------------------------------------------------

echo "-- Liveness --"
check_200 "GET /health"                "$HOST/health"
check_200 "GET /health/db"             "$HOST/health/db"
check_200 "GET /health/cache"          "$HOST/health/cache"

echo
echo "-- Sport health --"
check_200 "GET /api/v1/nba/health" "$HOST/api/v1/nba/health"

echo
echo "-- Entity products --"
check_200_or_404 "GET meta player=$PLAYER_ID"      "$HOST/api/v1/nba/player/$PLAYER_ID/meta"
check_200_or_404 "GET stats player=$PLAYER_ID"     "$HOST/api/v1/nba/player/$PLAYER_ID/stats"
check_200_or_404 "GET rating player=$PLAYER_ID"    "$HOST/api/v1/nba/player/$PLAYER_ID/rating"
check_200_or_404 "GET sigil player=$PLAYER_ID"     "$HOST/api/v1/nba/player/$PLAYER_ID/sigil"
check_200_or_404 "GET news player=$PLAYER_ID"      "$HOST/api/v1/nba/player/$PLAYER_ID/news"
check_200_or_404 "GET headlines player=$PLAYER_ID" "$HOST/api/v1/nba/player/$PLAYER_ID/headlines"
check_200_or_404 "GET sigil team=$TEAM_ID"         "$HOST/api/v1/nba/team/$TEAM_ID/sigil"

echo
echo "-- Sport data --"
check_200 "GET meta nba"              "$HOST/api/v1/nba/meta"
check_200 "GET autofill nba"          "$HOST/api/v1/nba/autofill"
check_200 "GET leaderboard nba"       "$HOST/api/v1/nba/leaderboard?limit=3"
check_200 "GET leaderboard sigil nba" "$HOST/api/v1/nba/leaderboard/sigil?limit=3"

echo
echo "-- CORS preflight --"
check_cors "OPTIONS /api/v1/nba/meta" "$HOST/api/v1/nba/meta" "$ORIGIN"
check_cors "OPTIONS entity meta" "$HOST/api/v1/nba/player/$PLAYER_ID/meta" "$ORIGIN"

echo
echo "-- Origin-less GET (server-to-server / SSR fetch path) --"
check_origin_less "GET /api/v1/nba/meta (no Origin)"     "$HOST/api/v1/nba/meta"
check_origin_less "GET /health/db (no Origin)"           "$HOST/health/db"

if [[ $FULL -eq 1 ]]; then
    echo
    echo "-- Write-through (--full) --"
    emit skip "--full" "no live provider-quota checks remain"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo
printf 'Summary: %s%d passed%s, %s%d failed%s, %s%d warned%s\n' \
    "$GREEN" "$PASSES" "$RESET" \
    "$RED" "$FAILS" "$RESET" \
    "$YELLOW" "$WARNS" "$RESET"

if [[ $FAILS -gt 0 ]]; then
    exit 1
fi
exit 0
