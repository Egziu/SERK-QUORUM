# Privacy architecture

Written so a customer's privacy reviewer can check each statement. "Implemented" means present in v0.2 code and covered by tests.

## What QUORUM is not

QUORUM is **not end-to-end encrypted**. The QUORUM server processes document and decision content in plaintext to store, search and export it. When a user sends a brief to an AI provider in live mode, that provider receives and processes the brief in plaintext under the customer's own agreement with that provider.

## Data inventory (v0.2)

| Data | Where | Protection today |
| --- | --- | --- |
| Email, display name | `users` | Visible only to the user and members of shared organizations (RLS) |
| Password | `users.password_hash` | Argon2id hash; plaintext never stored or logged |
| Session token | browser cookie; `sessions.token_digest` | HttpOnly cookie; only SHA-256 digest stored |
| Invitation token | shown once to inviter; `invitations.token_digest` | only digest stored |
| Projects, documents, versions, decisions | PostgreSQL | Tenant isolation (app + RLS); plaintext at rest unless the operator enables disk/volume encryption |
| Command briefs and responses | memory of the API process during the request | **Not stored** by QUORUM (only length and token counts) unless the user saves the response as a document |
| Usage metadata | `provider_requests` | provider, model, mode, outcome, prompt length, tokens, latency |
| Audit events | `audit_events` | event metadata only; no credentials, prompts or document content |
| Theme preference | browser `localStorage` | non-sensitive |
| Unsaved document draft (opt-in per document) | browser `sessionStorage` of that tab | cleared when the tab closes; user must tick a box to enable |

Secrets, tokens and confidential content are never written to `localStorage`. The CSRF token lives only in memory.

## External AI providers

* Off by default (`QUORUM_PROVIDERS_LIVE=0`). Fixture mode never contacts a provider.
* Each live request requires the user to tick an explicit consent naming the vendor and model; the server rejects live requests without that flag.
* The UI shows QUORUM's factual description of where the text goes. QUORUM does **not** summarize vendors' retention or training terms; customers must review their own agreements. `store: false` is sent to OpenAI and xAI Responses APIs.
* Keys are operator-provided environment variables, never sent to browsers, redacted from debug output.
* Customer data is not used to train any shared model by QUORUM. QUORUM performs no training.

## Encryption

| Layer | Status |
| --- | --- |
| Browser ↔ QUORUM | TLS is **required** in pilot/production but terminated by an operator-provided proxy; the API sets HSTS and `Secure` cookies when `QUORUM_COOKIE_SECURE=1`. |
| QUORUM ↔ AI providers | HTTPS with rustls; the client refuses plain HTTP (`https_only`) and redirects. |
| QUORUM ↔ PostgreSQL | TLS supported by the driver (rustls); enable with `sslmode=require`/`verify-full` in `DATABASE_URL`. |
| Data at rest | Not implemented in the application. Use encrypted volumes / managed-database encryption. |
| Field-level envelope encryption (AES-256-GCM, KMS-held KEKs) | *Planned* — see ARCHITECTURE §7. |
| Backups | Procedure verified unencrypted; encryption *planned* (BACKUP_AND_RECOVERY.md). |

## Retention and deletion

Not yet configurable. Versions and audit events are retained indefinitely; organization deletion cascades in the database but has no API or UI yet. Retention policies are planned before any pilot that holds personal data beyond account details.
