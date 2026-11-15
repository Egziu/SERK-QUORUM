#!/usr/bin/env bash
# Container smoke test: builds nothing, starts the given image next to a
# throwaway PostgreSQL, and checks migration, health, the web client, the
# non-root user and graceful shutdown. Synthetic configuration only.
#
#   scripts/container-smoke.sh serk-quorum:ci
set -euo pipefail

IMAGE="${1:?usage: container-smoke.sh IMAGE}"
NET="quorum-smoke-$$"
PG="quorum-smoke-pg-$$"
APP="quorum-smoke-app-$$"
PORT="${QUORUM_SMOKE_PORT:-18080}"

cleanup() {
  docker rm -f "$APP" "$PG" >/dev/null 2>&1 || true
  docker network rm "$NET" >/dev/null 2>&1 || true
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; docker logs "$APP" 2>&1 | tail -50 >&2 || true; exit 1; }

echo "• image runs as a non-root user"
user="$(docker image inspect -f '{{.Config.User}}' "$IMAGE")"
[[ -n "$user" && "$user" != "0" && "$user" != "root" ]] || fail "image user is '${user:-<unset>}'"

echo "• start PostgreSQL and create roles"
docker network create "$NET" >/dev/null
docker run -d --name "$PG" --network "$NET" -e POSTGRES_PASSWORD=smoke-superuser postgres:16 >/dev/null
for _ in $(seq 1 60); do docker exec "$PG" pg_isready -U postgres >/dev/null 2>&1 && break; sleep 1; done
docker exec -i "$PG" psql -U postgres -v ON_ERROR_STOP=1 -q < scripts/bootstrap-db.sql >/dev/null

echo "• migrate as the owner role"
docker run --rm --network "$NET" \
  -e QUORUM_MIGRATION_DATABASE_URL="postgres://quorum_owner:quorum_owner_dev@$PG:5432/quorum_dev" \
  "$IMAGE" migrate || fail "migration"

echo "• serve as the application role"
docker run -d --name "$APP" --network "$NET" -p "127.0.0.1:$PORT:8080" \
  -e DATABASE_URL="postgres://quorum_api:quorum_api_dev@$PG:5432/quorum_dev" \
  -e QUORUM_COOKIE_SECURE=0 \
  "$IMAGE" >/dev/null
for _ in $(seq 1 60); do curl -sf "http://127.0.0.1:$PORT/health/ready" >/dev/null && break; sleep 1; done
curl -sf "http://127.0.0.1:$PORT/health/live" >/dev/null || fail "liveness"
curl -sf "http://127.0.0.1:$PORT/health/ready" >/dev/null || fail "readiness"

echo "• process is not root inside the container"
uid="$(docker exec "$APP" id -u)"
[[ "$uid" != "0" ]] || fail "process runs as uid 0"

echo "• web client and security headers are served"
html="$(curl -sf "http://127.0.0.1:$PORT/")" || fail "GET /"
grep -q '<div id="root">' <<<"$html" || fail "index.html not served"
headers="$(curl -sfI "http://127.0.0.1:$PORT/o/any/connect")" || fail "client route fallback"
grep -qi '^content-security-policy:' <<<"$headers" || fail "missing CSP"
code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/api/v1/auth/session")"
[[ "$code" == "401" ]] || fail "anonymous session returned $code"
signup='{"email":"smoke@example.test","display_name":"Smoke","password":"synthetic-passphrase-smoke","organization_name":"Smoke"}'
body="$(curl -s -X POST -H 'content-type: application/json' -d "$signup" "http://127.0.0.1:$PORT/api/v1/auth/signup")"
grep -q '"code":"forbidden"' <<<"$body" || fail "public signup is not disabled by default: $body"

echo "• graceful shutdown on SIGTERM"
docker stop -t 20 "$APP" >/dev/null
exit_code="$(docker inspect -f '{{.State.ExitCode}}' "$APP")"
[[ "$exit_code" == "0" ]] || fail "exit code $exit_code after SIGTERM"
docker logs "$APP" 2>&1 | grep -q "shutdown signal received" || fail "no graceful shutdown log"

echo "CONTAINER SMOKE PASSED"
