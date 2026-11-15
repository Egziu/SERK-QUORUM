# Provider integrations

| Provider | Endpoint | Auth header | Request shape | Text extracted from |
| --- | --- | --- | --- | --- |
| OpenAI | `POST https://api.openai.com/v1/responses` | `Authorization: Bearer` | `model`, `input[]` (system guard + user), `max_output_tokens`, `store: false` | `output[type=message].content[type=output_text].text` |
| Anthropic | `POST https://api.anthropic.com/v1/messages` | `x-api-key`, `anthropic-version: 2023-06-01` | `model`, `system`, `messages[]`, `max_tokens` | `content[type=text].text` |
| Google Gemini | `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent` | `x-goog-api-key` | `systemInstruction`, `contents[]`, `generationConfig.maxOutputTokens` | `candidates[0].content.parts[].text` (parts with `thought: true` skipped) |
| xAI | `POST https://api.x.ai/v1/responses` | `Authorization: Bearer` | as OpenAI | as OpenAI |

References (carried over from v0.1, consulted for request/response shapes):
* https://developers.openai.com/api/reference/resources/responses/methods/create
* https://docs.anthropic.com/en/api/messages
* https://ai.google.dev/api/generate-content
* https://docs.x.ai/developers/rest-api-reference/inference/responses

## Verification status
Request construction and response parsing are tested against recorded-shape fixtures (`crates/quorum-providers/fixtures/`) and a local mock HTTP server. **No live vendor account was used in v0.2**, so schema drift at a vendor would only surface in a live smoke test. Before a pilot: run one synthetic request per configured provider, then add an explicit "verified" status backed by that check.

Model identifiers are configuration (`*_MODEL`). QUORUM does not assume any model exists; an invalid model returns a provider error (HTTP 4xx → `provider_failed`).
