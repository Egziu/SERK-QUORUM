# ADR 0001 — Modular monolith in Rust

* Status: Accepted (2026-11-14)

## Context
v0.1 was a dependency-free Node.js preview. QUORUM needs typed domain rules, tenant isolation, predictable resource use and a codebase reviewable by enterprise customers. Team size is small and load is unmeasured.

## Decision
One deployable Rust service (`quorum-api`, Axum/Tokio) with internal crate boundaries: `quorum-domain` (pure rules), `quorum-auth`, `quorum-storage`, `quorum-providers`. Application services stay in the API crate until a second delivery mechanism needs them. No microservices, message bus or cache until measurements justify them. The web client is a separate React/TypeScript SPA served by the same process.

## Consequences
+ One process to deploy, observe and secure; compile-time checks across layers; `unsafe` forbidden.
− Longer compile times; Rust hiring pool. Splitting services later requires extracting crates that are already separated.
