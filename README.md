# SERK QUORUM

![QUORUM logo](public/logo.svg)

**Intelligence. In concert.** · Multi-AI Engineering Framework

Independent commercial product direction: a human-led multi-model workspace for engineering firms, product teams and research organizations. v0.1 is a **local, single-operator engineering preview**, not a production SaaS.

## Start testing

Requires Node.js 22 or newer. No dependencies or provider keys needed for demo.

```sh
npm test
npm run check
npm start
```

Open http://127.0.0.1:8080. Compare four clearly labeled fixture perspectives, create session projects, record decisions and export JSON. Data is intentionally session-only; refresh clears it. No telemetry or background tasks.

## Optional live text adapters

Copy .env.example to .env; set QUORUM_LIVE=1, a strong random QUORUM_ACCESS_TOKEN (at least 32 characters), and the desired provider API key and model identifier. Start with `node --env-file=.env server.mjs`. Paste only the operator token into Connections. Select a provider, enter a non-sensitive brief and explicitly opt in to a live request. Configured does not mean verified. Keys remain server-side. API usage is billed separately by providers. Provider contracts and data policies must be reviewed before use.

Live requests are single-provider, text-only, operator-initiated, with a timeout, one concurrent request, a five-second cooldown and an output-token limit. Demo comparisons are fixtures, not generated responses. No live multi-agent execution, automated handoffs, offensive security functionality, production credentials, shell, network tools or repository mutation. The application is deliberately loopback-only. Do not expose it to untrusted users.

## Architecture

- `public/`: responsive black/green/white interface, reusable QUORUM emblem and session workspace.
- `lib/workspace.mjs`: provider registry, request validation, task catalog and demo fixtures.
- `lib/providers.mjs`: fixed official provider HTTP adapters and response normalization.
- `server.mjs`: local HTTP server, static assets, policy boundary and API.
- `test/`: unit and mocked HTTP/provider integration tests.
- `docs/PRODUCT.md`: commercial roadmap, security and release requirements.

The live APIs are not tested against real accounts in this release. Model IDs are configuration, not hard-coded promises. No production security certification is claimed.

## API references

- https://developers.openai.com/api/reference/resources/responses/methods/create
- https://docs.anthropic.com/en/api/messages
- https://ai.google.dev/api/generate-content
- https://docs.x.ai/developers/rest-api-reference/inference/responses

Copyright SERK Technologies Group. Commercial licensing terms are to be determined by the owner. No open-source license is implied.
