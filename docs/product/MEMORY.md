# Persistent project intelligence ("Memory") — design, not built

**Status: Planned.** Nothing in this document is implemented in v0.3, and the product does not show a Memory feature.

What v0.3 already provides as a foundation:

- Every outbound instruction is stored exactly, with its hash, brief version and the **versioned list of context items** (document/decision id, version, title) that went into it.
- Every imported answer is stored immutably with provenance, and every human judgement (review status, rating, annotation, decision) is attributed and audited.
- Decisions recorded from Council land in the Decision register and link back to their council.

Intended direction (subject to review before building):

1. **Approved knowledge only.** Memory would be built from approved documents and approved decisions, never from raw AI answers.
2. **Explicit context packages.** A reusable, versioned selection of approved items per project, chosen by people, replacing today's per-handoff selection.
3. **Retrieval with citations.** Any later suggestion must cite the exact item and version it relied on.
4. **Classification-aware.** Items above the organization's external sharing limit can never enter an outbound instruction.
5. **Deletion and retention.** Removing or superseding a source item must remove it from future context.

Open questions: embedding/indexing technology and where it runs, per-project access control (requires project membership, also planned), and evaluation of retrieval quality.
