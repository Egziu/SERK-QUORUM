# Dependency advisory exceptions

Each exception must state why it is not exploitable, how that was verified, and when it will be removed.

## RUSTSEC-2026-0459 — `libsqlite3-sys` 0.37.0 (SQLite CVE), medium

* **Why present:** recorded in `Cargo.lock` through `sqlx-sqlite`, an optional dependency of `sqlx` 0.9.0. Cargo records optional dependencies reachable through feature references even when they are not enabled.
* **Why not exploitable:** QUORUM does not enable sqlx's SQLite support. `cargo tree --target all -i libsqlite3-sys` prints nothing, i.e. the crate is not compiled for any target. QUORUM uses PostgreSQL only.
* **Why not upgraded:** `sqlx-sqlite` 0.9.0 requires a `libsqlite3-sys` range that excludes the fixed 0.39.0 (`cargo update -p libsqlite3-sys --precise 0.39.0` fails).
* **Recorded in:** `.cargo/audit.toml`.
* **Removal:** when sqlx publishes a release accepting the fixed version; re-check on every dependency update.
* **Recorded:** 2026-11-14.
