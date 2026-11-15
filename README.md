# SERK QUORUM

<img src="assets/quorum-logo.svg" alt="QUORUM four-node logo" width="72">

**Intelligence. In concert.** A vendor-independent organizational intelligence workspace: projects, versioned documents and a human-owned decision register, with explicit, text-only access to OpenAI, Anthropic, Google Gemini and xAI.

> **Status: v0.3 engineering preview — not production-ready.** The Rust foundation, the first vertical slice (sign-in → organization → project → document → decision record) and QUORUM Connect with Council (bring your own AI, no API key) are implemented and tested. See [RELEASE_READINESS.md](docs/RELEASE_READINESS.md) for exactly what is implemented, verified, partial and planned. Do not use with real customer data.

## What works today

| Area | Implemented (v0.3) |
| --- | --- |
| Identity | Local accounts (Argon2id), server-side sessions (HttpOnly cookie, revocation, expiry), CSRF protection, login throttling |
| Organizations | Multi-tenant organizations, 8 roles, invitations, role changes, last-owner protection |
| Tenant isolation | Application authorization **and** PostgreSQL row-level security; the API role owns no tables |
| Projects | Create, edit, status (active / on hold / completed / archived — archived is read-only) |
| Studio documents | 11 categories, append-only version history, optimistic concurrency, approval, Markdown export, AI-assisted labelling |
| Decision register | Draft → under review → approved / rejected → superseded, frozen content, full history, explicit human confirmation |
| Command | One provider, one text request; fixture mode (no network) or live with explicit consent; no silent fallback; usage ledger |
| Web Connect | Bring your own ChatGPT/Claude/Gemini/Grok account, no API key: structured briefs, role instructions, copy, open the official site yourself, paste the answer back (user-declared provenance, duplicates detected, originals immutable); organization classification limit for external sharing. See [WEB_CONNECT.md](docs/product/WEB_CONNECT.md) |
| Council | Compare responses side by side, annotate, rate on eight criteria, record the human decision into the register, export Markdown or a Studio document. See [COUNCIL.md](docs/product/COUNCIL.md) |
| Audit | Append-only (for the app role) audit trail of security and business events with correlation IDs |
| Web client | React + strict TypeScript; light/dark themes; responsive; WCAG 2.2 AA checks with axe-core |

Not yet built: OIDC/SSO, hybrid Council with API answers, Memory, Vault, Analytics, governance policies, billing, field encryption, PDF/DOCX export. These are tracked in [RELEASE_READINESS.md](docs/RELEASE_READINESS.md), not hidden behind placeholder screens.

## Quick start (local development, synthetic data)

Requirements: Rust 1.97 (pinned in `rust-toolchain.toml`; MSRV 1.88), Node.js 22+, PostgreSQL 16.

```bash
psql -v ON_ERROR_STOP=1 -f scripts/bootstrap-db.sql   # once, as a PostgreSQL superuser
cp .env.example .env
./scripts/dev.sh                                       # builds web + API, migrates, serves
```

Open http://127.0.0.1:8080 and create an organization. Fixture mode works without any provider keys.

Docker Compose alternative (definitions provided, **not yet verified in CI**): `docker compose -f deploy/docker/compose.yaml up --build`.

## Quality gates

```bash
export QUORUM_TEST_DATABASE_URL=postgres://quorum_owner:quorum_owner_dev@127.0.0.1:5432/postgres
./scripts/check.sh
```

Runs `cargo fmt --check`, `cargo clippy -D warnings`, all Rust tests (including PostgreSQL integration tests as the non-owner role), web typecheck/tests/build, and the legacy v0.1 suite. See [TESTING_GUIDE.md](docs/TESTING_GUIDE.md) for the browser acceptance test.

## Repository layout

```
apps/api          Axum HTTP service (handlers, sessions, tenant scope)
apps/web          React + TypeScript client (Vite)
crates/quorum-domain     Pure domain model: ids, roles, validation, lifecycles
crates/quorum-auth       Argon2id, opaque tokens, constant-time compare
crates/quorum-storage    PostgreSQL repositories, tenant-scoped transactions
crates/quorum-providers  OpenAI / Anthropic / Gemini / xAI adapters + fixtures
migrations/       SQL migrations (RLS, grants, triggers)
contracts/        OpenAPI 3.1 contract (kept in sync by tests)
docs/             Architecture, ADRs, security, operations, product
deploy/docker     Dockerfile and local Compose stack
legacy/v0.1-node  The original v0.1 Node preview, preserved and still tested
```

## Documentation

[Architecture](docs/ARCHITECTURE.md) · [Product vision](docs/product/PRODUCT_VISION.md) · [Security](SECURITY.md) · [Threat model](docs/security/THREAT_MODEL.md) · [Privacy architecture](docs/PRIVACY_ARCHITECTURE.md) · [Data model](docs/DATA_MODEL.md) · [API reference](docs/API_REFERENCE.md) · [Development](docs/DEVELOPMENT_GUIDE.md) · [Testing](docs/TESTING_GUIDE.md) · [Deployment](docs/DEPLOYMENT_GUIDE.md) · [Operations](docs/operations/OPERATIONS_RUNBOOK.md) · [Backup & recovery](docs/operations/BACKUP_AND_RECOVERY.md) · [Release readiness](docs/RELEASE_READINESS.md) · [ADRs](docs/adr/) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md)

## Boundaries

QUORUM is a planning, documentation and decision workspace. It contains no shell execution, autonomous agents, model-to-model loops, external target testing, exploit generation or automatic production changes. Model output is inert text reviewed by people; model agreement is not verification.

Copyright © SERK Technologies Group. Commercial licensing terms are to be determined by the owner; no open-source licence is granted.
