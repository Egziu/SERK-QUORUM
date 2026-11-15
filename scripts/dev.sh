#!/usr/bin/env bash
# Starts the API (serving the built web client) against the local database.
# Prerequisites: PostgreSQL running and scripts/bootstrap-db.sql applied once.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; [ -f .env ] && . ./.env; set +a
(cd apps/web && npm ci --silent && npm run build --silent)
cargo build -p quorum-api
./target/debug/quorum-api migrate
exec ./target/debug/quorum-api serve
