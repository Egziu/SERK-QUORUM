# Deployment guide

> No production deployment is authorized. v0.2 supports the **development** profile. The pilot and production profiles below are requirements, not achieved states.

## Profiles

### Development (supported)

* Local PostgreSQL via `scripts/bootstrap-db.sql`, or `deploy/docker/compose.yaml` (definitions written; image build **not yet verified** — no container runtime was available in the v0.2 session and CI does not build the image yet).
* Synthetic users only, fixture providers, `QUORUM_COOKIE_SECURE=0`, `QUORUM_ALLOW_SIGNUP=1`.

### Private pilot (requirements, not met yet)

* TLS 1.3 at a reverse proxy in front of the API; `QUORUM_COOKIE_SECURE=1` (default) so cookies are `Secure` and HSTS is sent.
* Private PostgreSQL with TLS (`sslmode=verify-full`), encrypted storage, automated backups with tested restore.
* Unique strong passwords for `quorum_owner` and `quorum_api` from a secret manager; the owner credential available only to the migration job.
* `QUORUM_ALLOW_SIGNUP=0`; organizations created by an operator; members by invitation.
* OIDC sign-in and MFA (not yet implemented — blocking).
* Live providers only after the customer's data-handling review; per-provider spend limits configured at the provider (QUORUM does not enforce spend caps).
* Single API replica until throttling moves to a shared store.
* Monitoring of `/health/ready`, error rates and logs; incident contact.

### Production (requirements)

Everything in pilot, plus: independent security assessment, approved privacy and commercial terms, documented SLOs measured over a pilot, restore drills, customer acceptance and human release authorization (see RELEASE_READINESS.md).

## Container image

```bash
docker build -f deploy/docker/Dockerfile -t serk-quorum:<version> .
docker run --rm -e QUORUM_MIGRATION_DATABASE_URL=… serk-quorum:<version> migrate
docker run -d -p 127.0.0.1:8080:8080 -e DATABASE_URL=… serk-quorum:<version>
```

The image runs as UID 10001, serves the SPA from `/app/web`, logs JSON, and exposes a health check. It is portable across container hosts; no cloud-specific services are required.

## Release process

1. All CI jobs green on the release commit.
2. Release readiness matrix reviewed; no release blocker open.
3. Migrations reviewed (no destructive change without explicit approval) and run by the migration job before the new API version starts.
4. Human approval recorded. Automatic production deployment is intentionally not configured.

## Graceful shutdown

The API stops accepting connections on SIGTERM/SIGINT and drains in-flight requests. Allow at least the provider timeout + 15 s (default 60 s) before forcing termination.

## Connecting AI providers (API mode) and testing them

Web Connect needs no configuration. For API mode:

1. Create an API key in the provider's developer console and choose an exact model identifier from its model list.
2. Set the server variables (on Railway: service → *Variables*), then let the service restart:

   | Variable | Example value |
   | --- | --- |
   | `QUORUM_PROVIDERS_LIVE` | `1` (master switch; `0` = fixtures only) |
   | `OPENAI_API_KEY`, `OPENAI_MODEL` | key, model id |
   | `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL` | key, model id |
   | `GEMINI_API_KEY`, `GEMINI_MODEL` | key, model id |
   | `XAI_API_KEY`, `XAI_MODEL` | key, model id |
   | `QUORUM_PROVIDER_MAX_OUTPUT_TOKENS` | `1200` (default) |
   | `QUORUM_PROVIDER_TIMEOUT_SECONDS` | `45` (default) |

   A provider needs both its key and its model; otherwise it shows *Not configured*.
3. Sign in as an owner or administrator, open **Providers & usage** and press **Test** for each provider. *Working* means the provider accepted the key and model for one tiny fixed request (≤ 16 output tokens, no customer data). Any other result names the cause (key rejected, model not found, rate limited / no quota, provider error, timeout, unreachable) and what to change. Results are audited and reset when the server restarts.
4. Use **Command** in *Live* mode for a real request; each one asks for explicit consent and is recorded in the usage ledger.

Enabling live mode sends text to external providers under your organization's agreements with them; review `docs/compliance/PROVIDER_POLICY_MATRIX.md` first.
