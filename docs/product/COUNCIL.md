# QUORUM Council (v0.3)

**Status: Implemented and verified** (integration tests, browser E2E on desktop and mobile).

Council is where people compare several AI responses to the same brief and make a decision. **Council never decides.** There is no automatic scoring, ranking, voting or "winner"; every rating and the decision itself are human acts, attributed and audited.

## Lifecycle

`draft` → `awaiting_responses` → `ready_for_review` ⇄ `under_review` → `decision_recorded` → `archived`

- Adding the first response moves a draft to *awaiting responses*; two or more make it *ready for review*. Other moves are explicit (`decisions_write`); archiving needs `decisions_decide`.
- **Recording the decision** (`decisions_decide`, from *ready for review* or *under review*, with an explicit confirmation) stores the summary, creates a **draft record in the Decision register** (problem = council question, alternatives = the responses with role/provider/provenance, evidence = the council report, risks = risk/assumption/contradiction annotations, decision maker = the person recording) and freezes the council. The register record then follows its normal human approval workflow.
- Completed councils are frozen in the application **and** in the database (trigger); archived is final.

## Comparison

- Up to 12 positions. Desktop shows responses side by side; phones default to one at a time with accessible tabs; the person can switch.
- Each response shows role, provider, declared model, size and a provenance badge (*User-declared* / *Provider-reported*) with its explanation.
- Replacing a position retires the previous response (kept in `retired` with `replaced_by`); removing retires without replacement. Responses must belong to the same project.
- Review status per response: unreviewed / accepted / rejected.
- Annotations: note, highlight, evidence, assumption, contradiction, alternative, question, risk — optionally quoting a passage. Authors can delete their own while the council is open.
- Ratings: eight criteria (evidence quality, requirements coverage, technical feasibility, clarity, cost implications, maintainability, identified risks, verification status), 1–5, one per person per criterion per response, with an optional reason. Other people's ratings are shown by name; nothing is averaged into a verdict.

## Export

- `GET …/councils/{id}/export.md` — Markdown with question, decision, every response with provenance, SHA-256, ratings and annotations, and a note that user-declared provenance is not verified by QUORUM.
- `POST …/councils/{id}/export-document` — saves the same report as a new Studio document marked AI-assisted, which then follows normal document review and approval.

PDF and DOCX are not implemented.
