# API reference (v1)

The machine-readable contract is [`contracts/openapi.json`](../contracts/openapi.json) (OpenAPI 3.1). Tests fail if it diverges from the router.

## Conventions

* Base path `/api/v1`. JSON only (`Content-Type: application/json` required on bodies). Unknown fields are rejected on most request bodies.
* **Authentication:** session cookie set by `/auth/login` or `/auth/signup` (`__Host-quorum_session` over HTTPS). **CSRF:** every non-GET request must send `x-csrf-token` with the value from `GET /auth/session`.
* **Tenancy:** organization resources live under `/orgs/{org}`. Non-members always receive `404`.
* **Correlation:** every response has `x-request-id` (client value echoed if `[A-Za-z0-9_-]{1,64}`).
* **Pagination:** `?limit=1..100` (default 50) `&offset=0..10000`; responses `{ items, limit, offset, has_more }`.
* **Concurrency:** document saves and decision edits/transitions require `base_version`; a mismatch returns `409`.
* **Limits:** 1 MiB request bodies; request timeout = provider timeout + 15 s.

## Error envelope

```json
{ "error": { "code": "validation_failed", "message": "title: must be between 1 and 200 characters",
             "field": "title", "correlation_id": "6f1c…" } }
```

| Status | `code` | Meaning |
| --- | --- | --- |
| 400 | `bad_request` | malformed JSON, unknown field, bad enum, bad pagination |
| 401 | `unauthenticated` | no or expired session; wrong credentials |
| 403 | `forbidden` / `csrf_failed` | role lacks permission / CSRF token missing or wrong |
| 404 | `not_found` | absent, or belongs to an organization you are not in |
| 409 | `conflict` | stale `base_version`, duplicate name, invalid status transition, archived project |
| 413 / 415 | `payload_too_large` / `unsupported_media_type` | |
| 422 | `validation_failed` | field-level validation (`field` set) |
| 429 | `rate_limited` | login throttle, provider cooldown or concurrency |
| 502 | `provider_failed` | live provider error; no fallback text is substituted |
| 500 | `internal` | logged with the correlation ID; no details returned |

## Endpoints

| Method & path | Permission | Notes |
| --- | --- | --- |
| `POST /auth/signup` | public (if `QUORUM_ALLOW_SIGNUP=1`) | creates user, organization, owner membership, session |
| `POST /auth/login` | public | throttled; generic errors |
| `POST /auth/logout`, `/auth/logout-all` | session | server-side revocation |
| `GET /auth/session` | session | user, memberships with permissions, CSRF token |
| `POST /orgs` | session | caller becomes owner |
| `GET /orgs/{org}` | `workspace_read` | includes caller role and permissions |
| `GET /orgs/{org}/members` | `workspace_read` | |
| `PATCH`/`DELETE /orgs/{org}/members/{user}` | `members_manage` | only owners touch owners; last owner protected |
| `GET`/`POST /orgs/{org}/invitations`, `DELETE …/{id}` | `members_manage` | token returned once on create |
| `POST /invitations/accept` | session | email must match invitation |
| `GET`/`POST /orgs/{org}/projects`, `GET`/`PATCH …/{project}` | read / `projects_manage` | |
| `GET`/`POST /orgs/{org}/projects/{project}/documents` | read / `documents_write` | |
| `GET /orgs/{org}/documents/{id}` and `/versions`, `/versions/{n}` | `workspace_read` | |
| `POST /orgs/{org}/documents/{id}/versions` | `documents_write` | new immutable version |
| `POST /orgs/{org}/documents/{id}/approve` | `documents_approve` | current version only |
| `GET /orgs/{org}/documents/{id}/export.md` | `workspace_read` | `text/markdown` attachment; audited |
| `GET`/`POST /orgs/{org}/projects/{project}/decisions` | read / `decisions_write` | |
| `GET`/`PATCH /orgs/{org}/decisions/{id}` | read / `decisions_write` | PATCH only in `draft` |
| `POST /orgs/{org}/decisions/{id}/transition` | `decisions_write` (submit/return/reopen) or `decisions_decide` (approve/reject/supersede, with `confirm: true`) | |
| `GET /orgs/{org}/decisions/{id}/versions` | `workspace_read` | |
| `GET /orgs/{org}/providers` | `workspace_read` | status: `disabled` / `not_configured` / `configured` (never "verified") |
| `POST /orgs/{org}/provider-runs` | `providers_use` | `mode: fixture | live`; live needs `acknowledge_external_transfer: true` |
| `GET /orgs/{org}/provider-runs` | `usage_read` | metadata only |
| `GET /orgs/{org}/audit-events` | `audit_read` | |
| `GET /health/live`, `/health/ready` | public | ready checks the database |

## Compatibility policy

Within `/api/v1`: fields may be added to responses and optional fields to requests. Removing or renaming fields, changing types or semantics, or tightening validation of existing fields is a breaking change and requires `/api/v2` and a changelog entry. Clients must ignore unknown response fields.
