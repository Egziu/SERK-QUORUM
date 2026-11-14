# Development guide

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Rust | 1.97.0 (pinned by `rust-toolchain.toml`) | minimum supported 1.88 (edition 2024, let-chains) |
| Node.js | 22 LTS or newer | web client and legacy preview |
| PostgreSQL | 16 | 15+ should work (uses `ON DELETE SET NULL (col)`); only 16 is tested |
| Docker | optional | Compose stack provided but not yet CI-verified |

## First run

```bash
# 1. Database roles + dev database (once, as a PostgreSQL superuser)
psql -v ON_ERROR_STOP=1 -f scripts/bootstrap-db.sql
# 2. Configuration
cp .env.example .env
# 3. Build web, build API, migrate, serve on http://127.0.0.1:8080
./scripts/dev.sh
```

Database roles (see ADR 0002):

* `quorum_owner` — owns the schema, runs `quorum-api migrate`. Never used by the running API.
* `quorum_app` — NOLOGIN group with the application's grants.
* `quorum_api` — the API's login role; member of `quorum_app`; owns nothing; no `BYPASSRLS`.

The passwords in `scripts/bootstrap-db.sql` and `.env.example` are local-development placeholders.

## Frontend development with hot reload

```bash
cd apps/web && npm ci && npm run dev    # Vite on :5173, proxies /api and /health to :8080
```

Run the API separately (`cargo run -p quorum-api -- serve` with `.env` loaded) and keep `QUORUM_COOKIE_SECURE=0` for plain HTTP.

## Working in the code

* Add a permission or role: `crates/quorum-domain/src/access.rs` (+ unit tests) and `apps/web/src/api/types.ts`; extend `apps/api/tests/authorization.rs`.
* Add an endpoint: handler in `apps/api/src/routes`, route in `app.rs` **and** `app::ROUTES`, contract in `contracts/openapi.json` (the contract tests enforce all three), authorization + isolation tests, audit event for state changes.
* Add a table: new migration file `migrations/000N_description.sql` with RLS enabled, a policy, explicit grants to `quorum_app`, and tenant isolation tests. Never edit applied migrations.
* Open a tenant transaction with `OrgScope::open(&state, &session, org, Permission::…)`; always also filter by `organization_id` in SQL.

## Configuration reference

All settings are environment variables; see `.env.example` for the full list and defaults. Notable defaults are secure: signup **off**, `Secure` cookies **on**, live providers **off**.

## Coding standards

See [CONTRIBUTING.md](../CONTRIBUTING.md). Clippy denies `unwrap_used`; `expect`/`panic` warn (and fail under `-D warnings`) outside tests.
