# Backup and recovery

## What must be backed up

Only PostgreSQL holds state. The API container is stateless; configuration lives in the deployment's secret store.

## Procedure (logical backup)

```bash
pg_dump -h <host> -U quorum_owner -Fc -f quorum-$(date -u +%Y%m%dT%H%M%SZ).dump quorum_dev
```

Run as the schema owner so policies, grants, functions and triggers are included. Store dumps encrypted (e.g. `age`/GPG or an encrypted bucket) — **QUORUM does not encrypt backups itself yet.**

## Restore

```bash
createdb -h <host> -U quorum_owner quorum_restore
pg_restore -h <host> -U quorum_owner -d quorum_restore --exit-on-error <file>.dump
```

The roles `quorum_owner`, `quorum_app` and `quorum_api` must already exist on the target cluster (`scripts/bootstrap-db.sql`), because grants reference them.

## Verified drill — 2026-11-14 (development data)

Dump of `quorum_dev` (66 KB, custom format) restored into a fresh database with `--exit-on-error`. Checks:

| Check | Source | Restored |
| --- | --- | --- |
| users / projects / document_versions / decision_versions / audit_events | 7 / 6 / 12 / 15 / 59 | 7 / 6 / 12 / 15 / 59 |
| RLS policies (`pg_policies`) | 18 | 18 |
| migration history rows | 1 | 1 |
| app role sees projects without tenant context | — | 0 rows (RLS active) |
| app role `DELETE FROM audit_events` | — | permission denied (grants restored) |
| owner `UPDATE` of approved decision | — | refused by trigger |

## Not yet in place

* Scheduled backups, retention schedule, off-site copies, point-in-time recovery (WAL archiving).
* Backup encryption and restore of encrypted backups.
* Automated restore test in CI.
* Recovery objectives (RPO/RTO) — to be set and measured during a pilot.
