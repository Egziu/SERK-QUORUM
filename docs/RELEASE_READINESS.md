# Release readiness matrix

Legend: **Verified** = implemented and covered by automated tests that ran green · **Implemented** = built, limited verification · **Partial** · **Planned** = not built. Updated 2026-11-14 for v0.2.

## First-release acceptance criteria (directive §35)

| Criterion | Status | Evidence |
| --- | --- | --- |
| Reproducible local installation | Implemented | `scripts/bootstrap-db.sql`, `scripts/dev.sh`, pinned toolchains/lockfiles; Compose file unverified |
| Working Rust backend | Verified | 78 Rust tests |
| Working TypeScript frontend | Verified | typecheck, 12 component tests, E2E |
| PostgreSQL persistence | Verified | integration tests on PostgreSQL 16 |
| Organization and user management | Verified | signup, invitations, roles, removal tests |
| Verified access-control boundaries | Verified | 8-role matrix, isolation (API + SQL + pool), mutation checks |
| Project creation and editing | Verified | API + E2E |
| Persistent decision records | Verified | lifecycle, history, immutability trigger |
| Document creation and version history | Verified | versions, concurrency, approval |
| Individual provider selection and response handling | Verified (mock) | adapters vs. recorded shapes + mock server; **live vendor APIs untested** |
| Clearly labelled fixture mode | Verified | API, UI, E2E |
| Secure server-side configuration | Implemented | secure defaults, redaction tests |
| Reliable exports for implemented formats | Verified | Markdown export test |
| Working automated tests | Verified | CI workflow; local runs recorded in TESTING_GUIDE |
| Complete installation documentation | Implemented | README, DEVELOPMENT_GUIDE |

## Capability matrix

| Capability | Status |
| --- | --- |
| Local accounts, sessions, CSRF, throttling | Verified |
| OIDC / SSO / MFA / account recovery | Planned |
| Organizations, 8 roles, invitations | Verified |
| Project-level access (per-project membership) | Planned (org-level roles only) |
| Tenant isolation (app + RLS) | Verified |
| Projects (status, archive) | Verified |
| Milestones, tasks, Kanban, timeline | Planned |
| Studio documents, versions, approval | Verified |
| Templates, duplication, PDF/DOCX export, branding | Planned |
| Decision register | Verified |
| Command (single provider) | Verified with mocks |
| Live provider verification call ("Verified" status) | Planned |
| Council (side-by-side comparison) | Planned |
| Memory (knowledge records, search) | Planned |
| Vault (attachments) | Planned |
| Analytics dashboards | Planned (usage ledger exists) |
| Governance policies, retention | Planned |
| Audit trail | Verified (append-only for app role; not tamper-evident) |
| Billing, entitlements, quotas | Planned |
| Field encryption / KMS | Planned |
| Backups | Partial — manual restore drill verified; no schedule, no encryption |
| Metrics / OpenTelemetry | Planned (structured logs + correlation IDs exist) |
| Container image | Implemented, unverified (no runtime in session; not built in CI) |
| SBOM, artifact signing | Planned |
| Performance benchmarks | Planned |

## Release blockers (directive §36) — current state

| Blocker | State |
| --- | --- |
| Known critical vulnerabilities | None known (cargo/npm audit clean; one documented medium exception, not compiled) |
| Unresolved authorization defects | None known |
| Cross-tenant data exposure | None known; tested at API and DB level |
| Unprotected secrets | Dev placeholders only; no secret manager integration — **blocker for pilot** |
| Failing required tests | None locally; GitHub CI result to be confirmed on the PR |
| Broken migrations | None |
| Unverified recovery | Manual drill only — **blocker for pilot** |
| Misleading privacy claims | Reviewed; no E2E-encryption claim |
| Undocumented provider data handling | Documented factually; vendor terms deferred to customer agreements |
| Unreviewed payment processing | No payment processing exists |
| Missing independent security assessment | **Open — blocks pilot and production** |

**Conclusion:** v0.2 is suitable for internal engineering review with synthetic data. It is **not** ready for a private pilot or production.
