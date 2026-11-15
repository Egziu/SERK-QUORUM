# Changelog

All notable changes are recorded here. Versions follow semantic versioning once 1.0 is released; before then, minor versions may break compatibility.

## [0.3.0] — unreleased (engineering preview)

### Added
- **QUORUM Connect — Web Connect:** use ChatGPT, Claude, Gemini and Grok with your own accounts and no API key. Quick and professional briefs with immutable versions; built-in and custom role templates; deterministic instruction rendering with person-selected, versioned project context; approved handoffs stored with SHA-256; copy and open-official-website steps; paste or file import (.txt, .md, QUORUM JSON) labelled user-declared; duplicate detection; review status.
- **Organization external sharing limit** by classification (public/internal/confidential/restricted), enforced server-side (`403 policy_blocked`).
- **Council:** compare up to 12 responses side by side (tabs on phones), replace with history, annotations, per-person ratings on eight criteria, human decision recording into the Decision register, frozen completed councils, Markdown export and save as Studio document.
- Migration `0002_web_connect_council.sql` with RLS, least-privilege grants, immutability triggers and provenance CHECK.
- CI `container` job: Docker image build, smoke test (`scripts/container-smoke.sh`) and CycloneDX SBOMs.
- Docs: WEB_CONNECT, COUNCIL, MEMORY (planned), PROVIDER_POLICY_MATRIX, ADR 0005, v0.3 baseline.

### Changed
- `PolicyBlocked` error code (`policy_blocked`, 403) added to the error envelope.
- Navigation adds **Connect**; projects gain a **Councils** tab.

## [0.2.0] — unreleased (engineering preview)

### Added
- Rust workspace (modular monolith): `quorum-domain`, `quorum-auth`, `quorum-storage`, `quorum-providers`, `quorum-api`.
- PostgreSQL schema with organization-scoped tables, row-level security, composite tenant foreign keys, append-only grants and decision immutability trigger.
- Local accounts (Argon2id), server-side sessions with revocation, CSRF protection, login throttling.
- Organizations, eight roles, invitations, member management.
- Projects; Studio documents with version history, approval and Markdown export; decision register with lifecycle and history.
- Command: single-provider text requests with fixture and live modes, explicit consent, usage records.
- Audit trail with correlation IDs; uniform error envelope.
- React + TypeScript web client with light/dark themes and responsive layout.
- OpenAPI 3.1 contract kept in sync with the router by tests.
- CI: fmt, clippy, Rust tests against PostgreSQL, cargo-audit, web typecheck/tests/build/npm audit, Playwright acceptance with axe-core, legacy suite.
- Dockerfile and local Compose stack (not yet verified by CI).
- Documentation set, ADRs, threat model, release readiness matrix.

### Changed
- The v0.1 Node preview moved to `legacy/v0.1-node/` unchanged; it still runs and its 9 tests still pass.
- The shared operator token of v0.1 is replaced by per-user accounts and sessions in the Rust service.

### Fixed
- Clearing a decision's review date (`"review_date": null`) was rejected with 400 (serde `flatten` limitation). Found by the browser acceptance test.

### Security
- Documented cargo-audit exception RUSTSEC-2026-0459 (uncompiled `libsqlite3-sys`); see `docs/security/DEPENDENCY_EXCEPTIONS.md`.

## [0.1.0] — 2026-11-14
- Local single-operator Node.js preview: four provider adapters, fixture comparison, session-only projects and decisions (commit `b175baa`).
