# ADR 0004 — Provider calls are single, consented, text-only requests

* Status: Accepted (2026-11-14)

## Context
QUORUM must stay separate from autonomous or offensive tooling and must never present fixtures as model output.

## Decision
`quorum-providers` exposes one operation: a single text completion to one provider chosen by the user. No tools, function calling, agents, retries across providers or model-to-model chaining. Live mode is off by default, requires server configuration and an explicit per-request consent flag. Failures surface as errors; fixtures are only returned when fixture mode is requested and are labelled in text, model id and UI. Prompts and responses are not stored; usage metadata is.

## Consequences
+ Small attack surface; transparent cost and data flow.
− Council comparisons must be assembled by people from separate requests or imported responses.
