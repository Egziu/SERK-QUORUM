# ADR 0003 — Server-side sessions now, OpenID Connect next

* Status: Accepted (2026-11-14)

## Context
The directive requires OIDC/SSO and MFA via the identity provider. No IdP is available to test against in this phase, and the vertical slice needs real per-user authentication replacing v0.1's shared operator token.

## Decision
Implement opaque, server-side sessions (random 256-bit token, SHA-256 digest stored, HttpOnly SameSite=Strict cookie, CSRF header, expiry, revocation). Local email/password accounts (Argon2id) are the first way to *establish* a session. OIDC (authorization code + PKCE, `iss`/`aud`/nonce validation) will be added as a second way to establish the same session, with MFA delegated to the IdP and per-organization enforcement of SSO. `users.password_hash` is nullable for IdP-only accounts. No JWTs are issued to browsers.

## Consequences
+ Immediate revocation; no token parsing in the browser; one session model for both login methods.
− Session lookup per request (indexed digest); local passwords must be disableable per organization once SSO exists.
