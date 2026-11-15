# QUORUM Connect — Web Connect (v0.3)

**Status: Implemented and verified** (API integration tests, component tests, browser E2E on desktop and mobile).

Web Connect lets a team consult ChatGPT, Claude, Gemini and Grok **through their own accounts in their own browser**, with no API key in QUORUM. QUORUM prepares the text and keeps the record; the person does the copying, opening and pasting.

## What QUORUM does and does not do

| QUORUM does | QUORUM never does |
| --- | --- |
| Builds a deterministic instruction from a brief, a role and the context the person selects | Contact, embed, frame or automate a provider website |
| Records the exact approved instruction and its SHA-256 | Read, scrape or extract provider conversations |
| Opens the provider's **official homepage** in a new tab when the person clicks (`target=_blank`, `rel="noopener noreferrer"`) | Put any project material in a URL, query string or fragment |
| Records that the person opened the site (a timestamp, nothing else) | Submit a prompt on the person's behalf |
| Stores the answer the person pastes or uploads, labelled *user-declared* | Ask for, store or intercept provider passwords, cookies or sessions |
| Detects exact duplicate imports | Claim that the person is signed in, subscribed or "connected" |

Official destinations (from the provider registry, tested): `https://chatgpt.com/`, `https://claude.ai/`, `https://gemini.google.com/`, `https://grok.com/`.

## Flow

1. **Project** — choose an active project.
2. **Brief** — *Quick* (one request) or *Professional* (objective, situation, desired outcome, background, constraints, requirements, deliverable format, evaluation criteria, open questions, audience, language). Each save appends an immutable version; edits use optimistic concurrency (`base_version`).
3. **Roles and instructions** — assign a role to each provider (defaults: Architect→ChatGPT, Builder→Claude, Scientist→Gemini, Challenger→Grok; any provider can take any role; organizations can add custom roles). Optionally select project documents and decisions as context; only those items are included, quoted as reference material with their version. *Prepare instructions* renders a preview and stores nothing. The person may edit each instruction, then **approves** it, which records a *handoff*.
4. **Copy, open and import** — per handoff: copy the instruction (clipboard, with a manual-copy fallback), open the official site, paste the answer (or import `.txt`, `.md`, or QUORUM JSON `{"format":"quorum.response.v1","text":"…","provider":"…","model":"…"}`, ≤ 1 MB). The model name and an optional `https://` conversation link are recorded as the person's declaration; QUORUM never opens the link.
5. **Compare in Council** — creates a Council with the imported responses in import order (see COUNCIL.md).

## States shown to people

`Ready to copy` → `Website opened` → `Response imported`. No state claims anything about the external account. API status is shown separately and is `Disabled`, `Not configured` or `Configured` (never "connected" or "verified").

## Policy and safeguards

- **Classification limit.** Every brief is `public`, `internal`, `confidential` or `restricted`. The organization's *external sharing limit* (default `internal`, changed only with `members_manage`, audited) is enforced on the server: a handoff for a brief above the limit is refused with `403 policy_blocked`. The UI warns before saving and before preparing.
- **Provenance.** Imported answers are `user_declared` with source `web_import` or `file_import`; only answers QUORUM itself obtains through an API can be `provider_reported` (a database CHECK ties the two).
- **Immutability.** The original response text, its hash, provider and provenance cannot be changed (database trigger); only the review status changes.
- **Duplicates.** Text is normalized (line endings, outer whitespace) and hashed; an identical answer in the same project returns `409 duplicate_response` with `existing_response_id`.
- **Input validation.** ≤ 200 000 characters, no control characters, provider from the known list or `other`, `https://` reference links only.
- **Audit.** `brief.created/updated`, `handoff.prepared` (provider, role, version, classification, edited flag, context count, hash), `handoff.website_opened`, `response.imported`, `response.reviewed`, `policy.external_share_limit_changed`. Audit details never contain instruction or answer text.
- **Permissions.** Prepare and import: `providers_use`. Read: `workspace_read`. Review status: `decisions_write` or `decisions_decide`. Custom roles: `projects_manage`.

## Not yet built

API Connect for the hybrid Council (saving Command/API answers as `provider_reported` responses), project-level confidentiality and project membership, a separate reusable context-package entity, and browser-extension or desktop helpers. Any automation of consumer AI websites is out of scope by policy.
