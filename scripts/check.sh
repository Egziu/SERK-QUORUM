#!/usr/bin/env bash
# Runs every local quality gate. Requires QUORUM_TEST_DATABASE_URL (see docs/TESTING_GUIDE.md).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
(cd apps/web && npm run typecheck && npm test && npm run build)
(cd legacy/v0.1-node && npm run check)
echo "All checks passed."
