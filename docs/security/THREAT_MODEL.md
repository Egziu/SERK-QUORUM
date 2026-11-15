# Threat model (v0.3)

Method: STRIDE per trust boundary (see ARCHITECTURE §3). Status: **Mitigated** (implemented and tested), **Partial**, **Open**. Reviewed by the implementing engineer only; an independent review is required.

## Assets

Tenant content (projects, documents, decisions), identities and sessions, provider API keys and spend, audit integrity, availability.

## Actors

Anonymous internet user; authenticated member of another tenant; insider with a low-privilege role; compromised browser session; malicious or manipulated AI output / pasted document; compromised dependency; database operator.

## Threats

| # | Area | Threat | Mitigation | Status |
| --- | --- | --- | --- | --- |
| T1 | Identity | Password guessing / stuffing | Argon2id, 12-char minimum, per-email throttle, generic errors, timing equalization | Partial — throttle is per process; no MFA, no breached-password check |
| T2 | Identity | Session theft via XSS | HttpOnly cookie, strict CSP without inline script, React escaping | Mitigated |
| T3 | Identity | Session theft via DB read | only SHA-256 digests stored | Mitigated |
| T4 | Identity | CSRF | per-session token header, SameSite=Strict, JSON-only bodies | Mitigated |
| T5 | Identity | Stolen session kept alive after password reset/compromise | revocation, logout-all, expiry | Partial — no password change/reset flow yet |
| T6 | Tenancy | Read another tenant's data by guessing IDs | membership check → 404; RLS; composite FKs; org filter in SQL | Mitigated (tests incl. confused-deputy paths) |
| T7 | Tenancy | Tenant context leaking via pooled connections | transaction-local `set_config` | Mitigated (pool-size-1 test) |
| T8 | Tenancy | Missing org filter in a new query | RLS as second layer | Mitigated for existing tables; process control for new ones (CONTRIBUTING) |
| T9 | Authorization | Privilege escalation via role change / invitation | `may_assign`, owner-only owner grants, last-owner guard | Mitigated |
| T10 | Authorization | Hidden UI control used directly | server-side enforcement on every route | Mitigated (8-role matrix test) |
| T11 | Integrity | Rewriting approved decisions or history | trigger + INSERT/SELECT-only grants | Mitigated for app role; Open for DB owner (not tamper-evident) |
| T12 | Repudiation | Actions without trace | audit events with actor, outcome, correlation ID, in same transaction | Partial — failed login events lack org context by design; no export/retention controls |
| T13 | Providers | Brief sent externally without consent | live off by default; consent flag required server-side | Mitigated |
| T14 | Providers | Prompt injection in pasted material makes the model act | no tools/actions exist; output is inert text; guard prompt; brief framed as untrusted | Mitigated (by absence of capability) |
| T15 | Providers | Model output rendered as HTML/script | rendered as text (`white-space: pre-wrap`), never `innerHTML` | Mitigated |
| T16 | Providers | Key leakage | env-only, redacted Debug, never sent to client | Mitigated; rotation manual |
| T17 | Providers | Spend abuse | permission `providers_use`, per-user cooldown, global concurrency, output token cap | Partial — no quota/budget per org |
| T18 | Providers | Silent substitution of fixture for failed live call | explicit 502, no fallback | Mitigated |
| T19 | Documents | Malicious uploads | no uploads exist yet | N/A (Vault planned with quarantine) |
| T20 | Input | SQL injection | static parameterized SQL only | Mitigated |
| T21 | Availability | Large bodies / slow requests | 1 MiB limit, timeouts, bounded provider responses, pagination caps | Partial — no global rate limiting |
| T22 | Supply chain | Vulnerable dependency | lockfiles, cargo/npm audit in CI, SHA-pinned actions | Partial — CycloneDX SBOMs generated in CI (v0.3); no signing or provenance attestation yet |
| T23 | Secrets | Secrets committed to the public repo | `.gitignore`, placeholders only, review | Partial — no automated secret scanning in CI yet |
| T24 | Backup | Backup disclosure | — | Open — backups not encrypted by QUORUM |
| T25 | Deployment | Plain HTTP in production | `Secure` cookies and HSTS default on; TLS at proxy | Partial — TLS is operator responsibility |
| T27 | Web Connect | Confidential material handed to an external AI | server-side classification limit (`policy_blocked`), UI warnings, audit of every handoff with classification | Mitigated for classified briefs; depends on people classifying correctly |
| T28 | Web Connect | Project data leaking through the provider URL, referrer or opener | fixed official homepages only, no query/fragment, `noopener noreferrer`, `Referrer-Policy: no-referrer` | Mitigated (component + E2E tests) |
| T29 | Web Connect | Phishing via a tampered destination link | destinations are constants in the provider registry, not data | Mitigated |
| T30 | Web Connect | Provider credentials or sessions captured | QUORUM never asks for, stores or reads them; no automation of provider sites | Mitigated (by absence of capability) |
| T31 | Integrity | Imported answer silently edited or misattributed | immutable original (trigger), SHA-256, provenance CHECK, `user_declared` label shown, audit | Mitigated; provider/model of imports remain a declaration |
| T32 | Integrity | Council outcome changed after the decision | frozen in app and DB trigger; register record follows its own immutability rules | Mitigated |
| T33 | Injection | Imported answer contains script or instructions | rendered as text; no tools act on it; included in later instructions only if a person selects approved context | Mitigated |
| T26 | Admin access | Operator reads tenant data with owner role | — | Open — requires separation of duties and break-glass auditing |

## Release-blocking items for a pilot

T1 (MFA via OIDC), T5, T12 (export/retention), T17 (org quotas), T22–T24, T26, plus an independent assessment.
