# QUORUM — commercial product charter

## Mission
Help engineering organizations convert diverse AI perspectives into transparent, evidence-based human decisions. The vendor-independent workspace is separate from any cybersecurity execution platform.

## Target customers
Software/product firms, architecture and construction engineering consultancies, research groups, and enterprise planning teams. Initial jobs: requirements preparation, architecture tradeoff documentation, project planning, supplied research synthesis and proposal review.

## Product layers
1. Experience: command center, responsive project spaces, accessible model selection, decision register.
2. Intelligence: provider adapter interface, explicit per-request routing, model identity, normalized outputs, transparent demo/live distinction.
3. Knowledge: session project index today; future tenant-scoped document repository, consent and versioned evidence.
4. Trust: human decision authority, provider opt-in, minimal permissions, data residency review, no execution tools.
5. Commercial: future organization workspaces, usage ledger, per-seat plans, enterprise private deployments.

## Roadmap and release gates
| Milestone | Scope | Exit criteria |
|---|---|---|
| 0.1 | Local preview | Unit and mocked HTTP tests pass; manual browser review; no external requests in demo |
| 0.2 | Private pilot | Real provider smoke tests with synthetic data; spend caps at provider; accessibility checks; reviewed privacy terms |
| 0.3 | Team workspace | OIDC, tenant isolation, PostgreSQL persistence, membership roles, audit events, deletion and export tests |
| 0.4 | Commercial pilot | Usage accounting, subscription entitlements, support and incident playbooks, independent security review |
| 1.0 | Production | Documented SLOs, restore drills, legal/privacy approval, external verification, customer acceptance |

## Engineering decisions
Use a dependency-free Node 22 preview for rapid reproducibility; later retain adapters behind a typed service boundary and move persistence to PostgreSQL. Do not confuse client-side session data with durable storage or audit evidence. Model responses are untrusted plain text; never execute them. Model agreement is not verification. Keep secrets server-side, avoid customer data during pilots, and do not enable live access on public networks. API limits are not hard financial caps; configure separate provider budgets.

## Commercial hypotheses
Team subscriptions by seat plus usage; customer-owned provider accounts for early pilots; enterprise private deployment and support. Pricing remains unvalidated. Measure task completion time, reviewer acceptance, provider reliability, cost per accepted deliverable and retention. Interview ten potential design partners before pricing commitments.

## Ownership
Product lead owns roadmap and customer research. Engineering lead owns architecture and releases. Design lead owns usability and accessibility. Platform engineer owns identity, persistence and deployment. Independent quality/security reviewer owns release evidence. Human customer administrator owns final decisions and data policies.

## Boundaries
No autonomous offensive agents, target testing, exploit generation, shell/command execution, external tool access, background agent loops or automatic repository/production actions. Live calls are one user-selected provider at a time. The prototype is not a hardened enterprise service.
