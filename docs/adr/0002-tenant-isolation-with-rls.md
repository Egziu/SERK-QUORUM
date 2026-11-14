# ADR 0002 — Tenant isolation: application checks plus PostgreSQL RLS

* Status: Accepted (2026-11-14)

## Context
A cross-tenant disclosure is release-blocking. Application checks alone fail open when one query forgets a filter.

## Decision
* Every tenant row carries `organization_id`; children reference parents through composite `(id, organization_id)` keys.
* The API opens a transaction per request and sets `app.user_id` / `app.org_id` with `set_config(…, true)` (transaction-local). RLS policies compare against these.
* The API connects as `quorum_api` (member of NOLOGIN `quorum_app`), which owns no tables and lacks `BYPASSRLS`. Migrations run as `quorum_owner`.
* Pre-tenant lookups (login, session, invitation) go through minimal `SECURITY DEFINER` functions.
* Version and audit tables are INSERT/SELECT-only for the app role.

## Consequences
+ Isolation holds even if application code omits a filter; verified by direct-SQL tests and a pool-size-1 leakage test.
− Every request uses a transaction; new tables need policies and grants (enforced by review and tests). Tables are not `FORCE`d, so the owner role bypasses RLS — owner credentials must be confined to migrations.
