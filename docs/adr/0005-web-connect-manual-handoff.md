# ADR 0005: Web Connect is a manual, person-driven handoff

Status: Accepted (v0.3)

## Context

Teams want to consult ChatGPT, Claude, Gemini and Grok with the accounts they already have, without giving QUORUM API keys. Automating consumer AI websites (headless browsers, extensions that read conversations, cookie reuse) would break provider terms, expose credentials and make provenance unverifiable.

## Decision

QUORUM prepares and records the exact instruction; the **person** copies it, opens the provider's official homepage in a new tab, and pastes the answer back. QUORUM never contacts the provider website. Imported answers are labelled `user_declared`; only answers QUORUM obtains itself through an API may be `provider_reported`, enforced by a database CHECK. Sharing is limited by an organization-level classification limit enforced on the server.

## Consequences

- Works with no keys and no automation; nothing to break when provider UIs change.
- Provenance of imported answers is a declaration, and the UI says so.
- More clicks for people than an integration would need; acceptable for decision-grade work where review matters more than speed.
