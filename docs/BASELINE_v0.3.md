# Phase A baseline before v0.3 (recorded 2026-11-15)

Recorded before any v0.3 change, on branch `claude/blissful-feynman-twemw7` at commit `2488c7c` (the v0.2 vertical slice plus platform `PORT` support). No newer upstream commits existed.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | pass |
| `cargo test --workspace --locked` (PostgreSQL 16, non-owner app role) | 79 passed, 0 failed, 0 ignored |
| `npm run typecheck && npm test` (apps/web) | pass; 12 passed |
| `npm run check` (legacy/v0.1-node) | 9 passed, 0 failed |

Known state at baseline:

- A private Railway deployment of v0.2 exists (created on the owner's explicit request). Public signup there must be turned off by the owner after creating their account. v0.3 does not deploy anything.
- The Docker image had been built successfully by Railway, but no CI job built or smoke-tested it. The sandbox used for development cannot reach Debian package mirrors, so local image builds fail at the runtime stage; v0.3 adds a CI container job instead (see TESTING_GUIDE).
- Live provider calls existed only behind `QUORUM_PROVIDERS_LIVE=1` with server-side keys; there was no way to use an AI without an API key.
