# Phase A baseline — v0.1 prototype

Recorded 2026-11-14 before any change.

| Item | Finding |
| --- | --- |
| Repository | `Egziu/SERK-QUORUM`; `main` at `f73c0b1` (README only) |
| Development branch | `quorum/v0.1-workspace` at `b175baa18ea94d1da4694c7fde6fbf8fc26df0f9` — matches the directive |
| Pull request | Egziu/SERK-QUORUM#1 "QUORUM v0.1 — independent intelligence workspace", draft, open, mergeable |
| CI | Workflow "QUORUM preview checks" (Node 22, `npm run check`); both runs on PR #1 succeeded |
| Local test run | `npm run check` on Node 22.23.4 → 9 passed, 0 failed (exit 0) |
| Implementation | Dependency-free Node HTTP server (`server.mjs`), four provider adapters (`lib/providers.mjs`), static SPA, fixture comparison, session-only projects/decisions in the browser |

## Limitations identified
1. Single shared operator token; no user accounts, organizations or roles.
2. No persistence — projects and decisions vanish on refresh.
3. Loopback-only by design; no TLS, sessions or CSRF model.
4. Global in-memory rate limiting; one concurrent live request server-wide.
5. No audit trail, usage ledger or version history.
6. Untyped request handling; validation ad hoc.
7. Provider adapters untested against live accounts (mocked only).

## Preservation
The prototype was moved unchanged to `legacy/v0.1-node/` and remains in CI. Its provider guard prompt, adapter shapes, response parsing and fixture texts were carried into `quorum-providers`. The logo is preserved at `assets/quorum-logo.svg` and `apps/web/public/logo.svg`.

## Branch note
The directive asked for branch `quorum/v0.2-rust-foundation`. This session's environment permits pushing only to `claude/blissful-feynman-twemw7`, which was fast-forwarded onto `b175baa` so the prototype history is preserved. The work can be pushed to `quorum/v0.2-rust-foundation` with the owner's authorization.
