# OWASP ASVS 5.0 and NIST SSDF alignment (self-assessment)

This is an engineering self-assessment to guide work, **not** a verification claim. Chapter names follow ASVS 5.0; requirement-level verification is future work for an independent assessor.

| ASVS 5.0 chapter | v0.2 status | Evidence / gap |
| --- | --- | --- |
| V1 Encoding & sanitization | Largely addressed | Parameterized SQL; React output encoding; no HTML rendering of user/AI text |
| V2 Validation & business logic | Addressed for slice | Domain validation, unknown-field rejection, lifecycle state machines, optimistic concurrency |
| V3 Web frontend security | Addressed | CSP, frame denial, nosniff, COOP, Referrer-Policy, same-origin API |
| V4 API & web service | Addressed | Versioned API, uniform errors, content-type enforcement, body limits |
| V5 File handling | N/A | No uploads yet |
| V6 Authentication | Partial | Argon2id, length policy, throttling; **no MFA, no recovery, no OIDC** |
| V7 Session management | Addressed | Opaque server-side sessions, rotation on login, revocation, expiry, HttpOnly/Secure/SameSite |
| V8 Authorization | Addressed | Deny-by-default RBAC, server-side checks, RLS defence in depth, tests |
| V9 Self-contained tokens | N/A | No JWTs used |
| V10 OAuth & OIDC | Not started | planned (ADR 0003) |
| V11 Cryptography | Partial | Vetted libraries only; no field encryption or key management yet |
| V12 Secure communication | Partial | rustls clients, HTTPS-only provider calls; inbound TLS delegated to proxy |
| V13 Configuration | Partial | Secure defaults, redacted config; no secret-manager integration |
| V14 Data protection | Partial | Minimal storage of prompts; no retention controls |
| V15 Secure coding & architecture | Addressed | `unsafe` forbidden, clippy `-D warnings`, dependency audit |
| V16 Security logging & error handling | Partial | Audit trail + correlation IDs; no alerting, no log integrity |
| V17 WebRTC | N/A | |

## NIST SSDF (SP 800-218) practices in use

* **PO** — security requirements in the directive and SECURITY.md; roles defined in CONTRIBUTING.
* **PS** — code in version control; lockfiles; SHA-pinned CI actions; least-privilege CI token.
* **PW** — threat model; reviewed dependencies; tests for security controls; static analysis via clippy; mutation spot-checks.
* **RV** — private vulnerability reporting route; documented dependency exceptions.

Gaps: SBOM generation, artifact signing/provenance, automated secret scanning, independent review.
