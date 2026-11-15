# Operations runbook

Scope: v0.2 single-instance deployments (development and future pilot). No SLOs are promised yet; targets below are engineering goals to be measured during a pilot.

## Health

| Endpoint | Meaning | Action if failing |
| --- | --- | --- |
| `GET /health/live` | process up; no dependencies | restart the container |
| `GET /health/ready` | database reachable | check PostgreSQL, credentials, connection limits |

## Logs

Structured `tracing` logs (`QUORUM_LOG_FORMAT=json` in containers). Every request span carries `correlation_id`, method and path. Users see the same ID in error messages ("ref …"); search logs for it. Logs never contain passwords, tokens, provider keys, prompts or responses.

## Common incidents

| Symptom | Likely cause | Steps |
| --- | --- | --- |
| All requests 500, ready fails | DB down or credentials wrong | `psql` as `quorum_api`; check `DATABASE_URL`; check `max_connections` |
| `permission denied for table …` in logs | new table without grants, or migration not applied | run `quorum-api migrate`; verify grants in migration |
| Users logged out unexpectedly | session TTL (`QUORUM_SESSION_TTL_HOURS`), clock skew, or `logout-all` | check `sessions.expires_at`/`revoked_at` for the user |
| 429 on login | 5 failures in 15 min for that email | wait, or restart the instance (in-memory state) after verifying it is not an attack |
| `provider_failed` | provider outage, wrong model id, quota, key revoked | check provider status page and key/model config; QUORUM never substitutes fixture text |
| Live mode unavailable | `QUORUM_PROVIDERS_LIVE` off or key/model missing | provider status in Providers & usage page |

## Access review

Organization owners review members and roles on the Members page. Audit events of type `membership.*`, `invitation.*` and `access.denied` show changes and refusals.

## Session revocation (suspected compromise)

* A user can sign out everywhere (`POST /api/v1/auth/logout-all`).
* Operator, for one user: `UPDATE sessions SET revoked_at = now() WHERE user_id = '<uuid>' AND revoked_at IS NULL;` as `quorum_owner`.
* All users: same without the user filter.

## Key and secret rotation

* Database passwords: create new password, update secret store, restart API.
* Provider keys: rotate at the provider, update environment, restart. Old key stops working at the provider.
* No application encryption keys exist yet.
