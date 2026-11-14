# Testing guide

Tests use deterministic synthetic data (`*@example.test`, "Atlas Engineering (synthetic)"). No test uses real credentials or contacts a real AI provider.

## Suites

| # | Category (directive §30) | Where | Count (v0.2) |
| --- | --- | --- | --- |
| 1–2 | Unit & domain invariants | `crates/*/src` `#[cfg(test)]`, `apps/api/src` | domain 20, auth 7, providers 12, api 10 |
| 3 | Database integration | `apps/api/tests/*` (real PostgreSQL, non-owner role) | 21 |
| 4 | Migrations | `database::migrations_are_recorded_and_idempotent`; every test migrates a fresh DB | incl. above |
| 5 | API contract | `apps/api/tests/contract.rs` (spec ↔ route list ↔ router) | 2 |
| 6 | Authorization | `apps/api/tests/authorization.rs` (all 8 roles × 8 actions) | 4 |
| 7 | Tenant isolation | `apps/api/tests/tenant_isolation.rs` (API, direct SQL, pool leakage) | 4 |
| 8 | Cryptography vectors | SHA-256 FIPS 180-2 "abc" vector; Argon2id round-trip (`quorum-auth`) | 2 |
| 9 | Provider fixtures | `crates/quorum-providers` (recorded-shape JSON + local mock HTTP server) | 18 |
| 10 | Frontend components | `apps/web/src/**/*.test.ts(x)` (Vitest + Testing Library) | 12 |
| 11–12 | Browser E2E + accessibility | `apps/web/e2e/run.mjs` (Playwright + axe-core, WCAG 2.0–2.2 A/AA rules) | 1 flow, 12 page states |
| 13 | Backup restoration | manual drill, see BACKUP_AND_RECOVERY.md | not automated |
| 14 | Billing arithmetic | — | not applicable yet (no billing) |
| 15 | Performance benchmarks | — | **not yet** |
| — | Legacy v0.1 | `legacy/v0.1-node/test` | 9 |

## Running

```bash
# Database-backed tests need an owner-role URL with CREATEDB (each test creates and drops its own DB):
export QUORUM_TEST_DATABASE_URL=postgres://quorum_owner:quorum_owner_dev@127.0.0.1:5432/postgres
# optional: QUORUM_TEST_APP_USER / QUORUM_TEST_APP_PASSWORD (default quorum_api / quorum_api_dev)

cargo test --workspace                 # all Rust tests
cd apps/web && npm test                # component tests
./scripts/check.sh                     # everything except E2E
```

Database tests **fail, not skip,** when `QUORUM_TEST_DATABASE_URL` is missing, so an isolation test can never silently not run. A test that panics may leave a `quorum_test_*` database behind; drop them with `psql -c "\l quorum_test_*"` then `DROP DATABASE`.

### Browser acceptance

```bash
cd apps/web && npm run build
# start the API with QUORUM_ALLOW_SIGNUP=1 QUORUM_COOKIE_SECURE=0 QUORUM_WEB_DIR=apps/web/dist (see CI)
npm run e2e        # QUORUM_E2E_URL defaults to http://127.0.0.1:8080
```

The run fails on: any failed step, any browser console error, any CSP violation, any HTTP ≥ 400 other than the anonymous session probe, horizontal overflow on a phone viewport, and any *serious* or *critical* axe-core finding. Screenshots go to `apps/web/test-results/screenshots` (CI uploads them as an artifact).

## Evidence from the v0.2 session (2026-11-14)

All commands exited 0 unless noted.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --workspace` | 78 passed, 0 failed, 0 ignored |
| `npm run typecheck && npm test && npm run build` (apps/web) | 12 passed; build OK |
| `node e2e/run.mjs` | E2E PASSED; 12 page states, 0 axe violations |
| `npm run check` (legacy/v0.1-node) | 9 passed |
| `cargo audit` | pass with one documented exception (RUSTSEC-2026-0459) |
| `npm audit --audit-level=high` | 0 vulnerabilities |

### Defects found by the tests during development

* Browser E2E found that clearing a decision review date returned 400 (serde `flatten` + custom deserializer). Fixed; regression test added.
* axe-core found scrollable tables not reachable by keyboard (`scrollable-region-focusable`). Fixed with focusable labelled regions.
* Screenshot review found the mobile menu button visible on desktop (CSS order). Fixed.

### Mutation checks (do the tests detect real regressions?)

Performed manually and reverted:

* Disabled the permission check in `OrgScope::open` → `authorization::every_role_is_enforced_server_side` failed.
* Removed `ENABLE ROW LEVEL SECURITY` on `projects` → `tenant_isolation::row_level_security_enforces_isolation_in_the_database` and `pooled_connections_do_not_leak_tenant_context` failed, while the API-level isolation test still passed (application checks are an independent layer).
