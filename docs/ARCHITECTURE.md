# Architecture

This document describes what is **implemented in v0.2**. Future architecture is labelled *Planned* and drawn with dashed lines. Diagrams use Mermaid and render on GitHub.

## 1. System context

```mermaid
flowchart LR
  user([Organization member<br/>browser])
  admin([Operator])
  subgraph Q[SERK QUORUM]
    web[Web client<br/>React SPA]
    api[quorum-api<br/>Rust / Axum]
    db[(PostgreSQL 16)]
  end
  oai[OpenAI API]
  ant[Anthropic API]
  gem[Google Gemini API]
  xai[xAI API]
  idp[[Identity provider<br/>OIDC — planned]]

  user -- HTTPS --> web
  web -- same-origin JSON /api/v1 --> api
  api -- SQL over TLS-capable connection --> db
  api -. text brief, only with explicit consent .-> oai & ant & gem & xai
  admin -- env config, migrations --> api
  api -.-> idp
```

## 2. Logical architecture (modular monolith)

One deployable binary with internal boundaries. Crates depend inward on the domain; the domain depends on nothing with I/O.

```mermaid
flowchart TB
  subgraph experience[Experience plane]
    web[apps/web]
  end
  subgraph app[apps/api — application services]
    routes[routes/*<br/>auth · orgs · projects · documents · decisions · providers · audit]
    ctx[context<br/>Session extractor · OrgScope · pagination]
    mw[middleware<br/>request id · security headers · limits · timeouts]
  end
  subgraph core[Domain]
    domain[quorum-domain<br/>ids · roles/permissions · validation · lifecycles]
  end
  subgraph infra[Infrastructure]
    storage[quorum-storage<br/>repositories · tenant transactions · migrations]
    auth[quorum-auth<br/>Argon2id · tokens]
    prov[quorum-providers<br/>4 adapters · fixtures]
  end
  web --> routes
  mw --> routes --> ctx
  routes --> domain
  routes --> storage & auth & prov
  storage --> domain
```

| Plane (directive §04) | v0.2 realisation |
| --- | --- |
| Experience | `apps/web` |
| Identity & access | `quorum-auth`, `context::Session`, `context::OrgScope`, `quorum-domain::access` |
| Application services | `apps/api/src/routes` |
| Provider integration | `quorum-providers` |
| Knowledge & documents | documents + versions (storage, routes); Memory/Vault *planned* |
| Data protection | RLS, grants, hashing; field encryption *planned* (§7) |
| Governance & audit | audit events, roles; policies *planned* |
| Commercial | usage ledger (`provider_requests`); entitlements/billing *planned* |
| Observability & operations | `tracing` structured logs, correlation IDs, health endpoints; metrics/OTel *planned* |

Application services currently live in the API crate rather than a separate `quorum-application` crate; extraction is deferred until a second delivery mechanism (worker, CLI) needs them (ADR 0001).

## 3. Request lifecycle and trust boundaries

```mermaid
sequenceDiagram
  autonumber
  participant B as Browser
  participant M as Middleware
  participant H as Handler
  participant S as Session extractor
  participant O as OrgScope
  participant P as PostgreSQL
  B->>M: POST /api/v1/orgs/{org}/projects (cookie, x-csrf-token, JSON)
  M->>M: assign x-request-id · 1 MiB body limit · timeout
  M->>H: route
  H->>S: extract session
  S->>P: auth_find_session(sha256(token)) [SECURITY DEFINER]
  S->>S: constant-time CSRF check (non-GET)
  H->>O: open(org, Permission::ProjectsManage)
  O->>P: BEGIN; set_config(app.user_id, app.org_id, local)
  O->>P: role_of(org, user)  — none → 404, insufficient → 403 (+ audit)
  H->>P: INSERT … (RLS WITH CHECK org = app.org_id)
  H->>P: INSERT audit_events
  H->>P: COMMIT (context discarded)
  H-->>B: 201 JSON + security headers + x-request-id
```

Trust boundaries:

```mermaid
flowchart LR
  subgraph untrusted[Untrusted]
    browser[Browser input]
    llm[Provider responses]
  end
  subgraph tb1[API process — validated & authorized]
    api[quorum-api]
  end
  subgraph tb2[Database — RLS + grants as second line]
    db[(PostgreSQL)]
  end
  subgraph ext[External processors]
    providers[AI providers]
  end
  browser -->|validate · authenticate · CSRF · authorize| api
  api -->|tenant context · least privilege role| db
  api -->|consented brief only| providers
  providers -->|inert text, size-bounded, never executed| api
  llm -.-> api
```

## 4. Identity architecture

* **Implemented:** local accounts (email + Argon2id), server-side sessions in `sessions` (SHA-256 digest of a 256-bit token, CSRF token, expiry, revocation). Login/session/invitation lookups happen through narrowly scoped `SECURITY DEFINER` SQL functions because no tenant context exists yet at that point.
* **Planned:** OpenID Connect (authorization code + PKCE) as an additional way to *establish* the same session; MFA delegated to the IdP; SCIM or just-in-time provisioning; account recovery. See ADR 0003.

## 5. Organization isolation

```mermaid
flowchart TB
  req[Request for org X] --> m{membership of user in X?}
  m -- no --> nf[404 — existence not disclosed]
  m -- yes --> p{role grants permission?}
  p -- no --> fb[403 + access.denied audit]
  p -- yes --> tx[transaction with app.org_id = X]
  tx --> q[queries filter organization_id = X]
  q --> rls[RLS: organization_id = app_current_org]
  rls --> fk[composite FKs: child.org = parent.org]
```

Three independent layers must all fail for a cross-tenant read or write. Integration tests exercise each layer separately, including with RLS deliberately disabled (mutation check recorded in the testing guide).

## 6. Provider integration

`quorum-providers` builds one HTTP request per call (OpenAI and xAI Responses API, Anthropic Messages API, Gemini `generateContent`), with a fixed guard system prompt, the user brief framed as untrusted material, `store: false` where supported, a bounded output token count, timeout and a 2 MiB response cap. There is no tool calling, function calling, streaming to tools, or chaining between models. Fixture mode returns labelled static text and is never substituted for a failed live call.

## 7. Key management (*Planned*)

No field-level encryption exists in v0.2. Provider keys are environment variables of the API process; passwords and tokens are hashed, not encrypted.

```mermaid
flowchart LR
  kms[[External KMS / Vault<br/>key-encryption keys]]:::planned
  api[quorum-api]:::planned
  dek[Per-tenant data keys<br/>wrapped by KEK, versioned]:::planned
  db[(Encrypted columns<br/>AES-256-GCM + AAD = org,table,column,row)]:::planned
  api -- unwrap DEK --> kms
  api -- encrypt/decrypt --> db
  dek --- db
  classDef planned stroke-dasharray: 5 5
```

See [PRIVACY_ARCHITECTURE.md](PRIVACY_ARCHITECTURE.md) for what is and is not encrypted today.

## 8. Document lifecycle

```mermaid
stateDiagram-v2
  [*] --> Draft: create (version 1)
  Draft --> Draft: save → new immutable version (base_version must match)
  Draft --> Approved: approve current version (documents_approve)
  Approved --> Draft: save → new version (approved version stays in history)
  note right of Approved: Export to Markdown at any version; AI-assisted versions are labelled
```

Decision lifecycle: see `crates/quorum-domain/src/decision.rs` (state diagram in the module docs) and [DATA_MODEL.md](DATA_MODEL.md).

## 9. Deployment

```mermaid
flowchart LR
  subgraph host[Container host — implemented as definitions, not yet CI-verified]
    api[quorum-api container<br/>non-root, serves SPA + API]
    mig[quorum-api migrate<br/>one-shot, owner role]
  end
  tls[[TLS-terminating reverse proxy<br/>required, operator-provided]]
  pg[(PostgreSQL 16<br/>managed or self-hosted)]
  users([Users]) -- HTTPS --> tls --> api
  api -- quorum_api role --> pg
  mig -- quorum_owner role --> pg
```

Profiles (development / private pilot / production) are described in [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md).

## 10. Technology choices

Rust 1.97 (MSRV 1.88), Axum 0.8, Tokio 1, sqlx 0.9 (runtime queries, Postgres, rustls), tower-http 0.7, reqwest 0.13 (rustls), argon2 0.6, tracing. Web: React 19, React Router 7, TypeScript 5.9 strict, Vite 8, Vitest 5, Playwright 1.63, axe-core 4.14. Versions are pinned in `Cargo.lock` and `package-lock.json`. Redis, object storage, pgvector and OpenTelemetry are not used yet; each will be introduced only with a measured need.
