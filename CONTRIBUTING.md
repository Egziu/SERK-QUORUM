# Contributing

## Ground rules

* Work on a branch; open a draft pull request early. Never force-push shared branches or merge without the required review.
* Commits are focused and use conventional prefixes: `feat:`, `fix:`, `test:`, `docs:`, `build:`, `refactor:`, `chore:`.
* Never commit secrets, real customer data, certificates or internal SERK project material. Tests and fixtures use synthetic data only (`*@example.test`).
* Do not describe planned work as implemented. Update `docs/RELEASE_READINESS.md` when a capability's status changes.

## Before pushing

```bash
./scripts/check.sh   # fmt, clippy -D warnings, all tests, web build, legacy suite
```

and, for UI changes, the browser acceptance test (see `docs/TESTING_GUIDE.md`).

## Engineering standards

* Rust: no `unsafe` (forbidden workspace-wide); no `unwrap`/`expect`/`panic` in production code; typed errors; validate at trust boundaries; static parameterized SQL only; checked arithmetic for counts, money and quotas.
* Every tenant-data query runs inside `quorum_storage::begin` with the organization context **and** filters by `organization_id`. New tables need RLS policies, grants for `quorum_app`, and isolation tests.
* New endpoints: add to `app::ROUTES`, `contracts/openapi.json`, authorization tests and, where relevant, audit events.
* Migrations are append-only; never edit an applied migration (sqlx verifies checksums). Destructive migrations need explicit approval.
* Frontend: strict TypeScript, no inline scripts/styles (CSP), accessible names for every control, keyboard operability, AA contrast.
* Architecture decisions that are hard to reverse get an ADR in `docs/adr/`.

## Scope

QUORUM excludes shell execution, autonomous agents and agent chains, external target testing, exploit generation, unrestricted tool access and automatic production changes.
