# Security policy

## Status

SERK QUORUM v0.2 is an **engineering preview**. It has not had an independent security assessment and must not hold real customer data. A successful build or green CI does not establish production security.

## Reporting a vulnerability

Do not open a public issue. Report privately to the repository owner through GitHub's private vulnerability reporting ("Security" tab → "Report a vulnerability"). Include affected version/commit, reproduction steps with synthetic data, and impact. Cross-tenant data exposure is treated as release-blocking.

## Implemented controls (v0.2)

| Control | Implementation | Verified by |
| --- | --- | --- |
| Password storage | Argon2id, 19 MiB / t=2 / p=1 (OWASP baseline), per-hash salt | `quorum-auth` unit tests; `database::secrets_are_never_stored_in_plaintext` |
| Sessions | 256-bit random token in `HttpOnly; SameSite=Strict` cookie (`__Host-` + `Secure` when HTTPS); only SHA-256 digest stored; expiry; server-side revocation; logout-all | `vertical_slice::signup_login_logout_and_session_revocation` |
| CSRF | Per-session token required in `x-csrf-token` on every non-GET; JSON content type required | `vertical_slice::state_changes_require_csrf_token_and_json` |
| Login abuse | Generic errors, timing equalization for unknown accounts, 5 failures / 15 min throttle per email (in-process) | `vertical_slice::login_is_throttled_after_repeated_failures` |
| Authorization | Deny-by-default role → permission map enforced in every handler; non-members get 404 | `authorization::every_role_is_enforced_server_side` |
| Tenant isolation | Org-scoped queries + PostgreSQL RLS with transaction-local context; composite tenant foreign keys; API role owns nothing, no `BYPASSRLS` | `tenant_isolation::*` (API, direct SQL, pooled-connection leakage) |
| History integrity | App role has INSERT/SELECT only on audit events and version tables; trigger blocks edits to non-draft decisions (including by the owner role) | `database::application_role_cannot_rewrite_history_or_audit`, `database::approved_decisions_are_immutable_even_to_direct_sql` |
| Input validation | Typed DTOs with unknown-field rejection, length and control-character checks, 1 MiB body limit | domain unit tests, API tests |
| SQL injection | Static, parameterized SQL only (sqlx 0.9 accepts only `&'static str` queries) | code review; no dynamic SQL in application crates |
| Output / browser | Strict CSP (`script-src 'self'`, no inline), `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, COOP, Permissions-Policy, HSTS when Secure; React escaping, no `dangerouslySetInnerHTML` | `vertical_slice::health_endpoints_and_security_headers`; e2e fails on CSP violations |
| Errors | Uniform envelope with correlation ID; no SQL errors, stack traces or provider bodies returned | API tests |
| Provider safety | Text only; no tools; guard prompt; brief framed as untrusted; consent required for live; no silent fixture fallback; keys server-side, redacted in `Debug`; response size bounded | `quorum-providers` tests, `providers::*` |
| Secrets in logs | Config `Debug` redacts DB URL and keys; prompts and responses are not logged or stored | `config::tests::debug_output_redacts_secrets`, `providers::fixture_runs_are_labelled_and_recorded_without_prompt_text` |
| Supply chain | Lockfiles committed; `cargo audit` and `npm audit` in CI; Actions pinned by SHA; read-only `GITHUB_TOKEN` | CI |
| Memory safety | `unsafe_code = "forbid"` across first-party crates | compiler |

## Known gaps (must be closed before any production use)

* No OIDC/SSO or MFA yet; local passwords only. No account recovery flow.
* Throttling and provider rate limits are per process (not shared across replicas).
* No field-level encryption, KMS integration or encrypted backups.
* Audit records are append-only *for the application role only*; the schema owner and DBAs can still alter them. They are not tamper-evident.
* Signup reveals whether an email is registered (409). Acceptable for invitation-led pilots; revisit for public signup.
* Invitations are delivered out of band (no email service); the token is shown once to the inviter.
* No independent penetration test or ASVS assessment has been performed. See [docs/security/THREAT_MODEL.md](docs/security/THREAT_MODEL.md) and [docs/security/ASVS_MAPPING.md](docs/security/ASVS_MAPPING.md).
* Dependency exception: see [docs/security/DEPENDENCY_EXCEPTIONS.md](docs/security/DEPENDENCY_EXCEPTIONS.md).

## Scope boundaries

QUORUM deliberately excludes shell execution, autonomous agents, external target testing, exploit generation, unrestricted tool access and automatic production changes. Contributions adding such capabilities will not be accepted.
