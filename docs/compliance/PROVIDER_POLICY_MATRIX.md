# Provider policy matrix (v0.3)

This matrix records **what QUORUM does** with each provider and what still needs review. It is not legal advice and does not summarize any provider's terms: those change, and the organization using QUORUM must review the current terms of service, usage policies and data-processing terms of each provider and of each plan its people use.

## Web Connect (no API key)

| | ChatGPT (OpenAI) | Claude (Anthropic) | Gemini (Google) | Grok (xAI) |
| --- | --- | --- | --- | --- |
| Destination opened | `https://chatgpt.com/` | `https://claude.ai/` | `https://gemini.google.com/` | `https://grok.com/` |
| How it opens | New tab after an explicit click; `noopener noreferrer`; no query string or fragment | same | same | same |
| Data QUORUM sends to the provider | **None.** The person pastes the instruction themselves | None | None | None |
| Automation, scraping, framing, cookie or credential handling | None (forbidden by policy and absent from code) | None | None | None |
| What QUORUM stores | Approved instruction + SHA-256, the person's pasted answer, declared model, optional https link (never fetched) | same | same | same |
| Provenance label | `user_declared` | `user_declared` | `user_declared` | `user_declared` |
| Classification guard | Brief must be ≤ organization limit (default `internal`) | same | same | same |

Responsibilities that stay with the organization and the person:

- Whether their account type (consumer, team, enterprise) may be used for the organization's work, and what that provider may retain or train on for that account type.
- Whether the material in a brief may be shared with that provider at all; QUORUM's classification limit is a guard, not a substitute for this judgement.
- Not pasting secrets, credentials or personal data into briefs.

## API mode (Command)

| | OpenAI | Anthropic | Google Gemini | xAI |
| --- | --- | --- | --- | --- |
| Default | Disabled (`QUORUM_PROVIDERS_LIVE=0`) | Disabled | Disabled | Disabled |
| Enabled only when | Live mode on **and** server-side key **and** model configured, and the person acknowledges the external transfer per request | same | same | same |
| Credentials | Server environment only; never sent to the browser or stored in the database | same | same | same |
| Status shown | Disabled / Not configured / Configured (unverified) | same | same | same |
| Data-handling note shown in UI | From the provider registry (`data_handling`) | same | same | same |

## Review status

| Item | Status |
| --- | --- |
| Engineering controls above | Implemented and tested |
| Legal review of each provider's current terms for the intended account types | **Not done** — required before any production use with real data |
| Data-processing agreements for API use | **Not done** |
| Review of this matrix | Owner of the deployment, before enabling live API mode or onboarding real users |
