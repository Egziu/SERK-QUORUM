# syntax=docker/dockerfile:1
# Multi-stage build: web client, API binary, minimal non-root runtime.
# Build from the repository root:
#   docker build -f deploy/docker/Dockerfile -t serk-quorum:dev .

FROM node:22-bookworm-slim AS web
WORKDIR /src/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY apps/web/ ./
RUN npm run build

FROM rust:1.97-bookworm AS api
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY apps/api/ apps/api/
COPY migrations/ migrations/
RUN cargo build --release --locked -p quorum-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /app quorum
WORKDIR /app
COPY --from=api /src/target/release/quorum-api /app/quorum-api
COPY --from=web /src/apps/web/dist /app/web
ENV PORT=8080 \
    QUORUM_WEB_DIR=/app/web \
    QUORUM_LOG_FORMAT=json
USER 10001
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --retries=5 CMD curl -fsS http://127.0.0.1:8080/health/ready || exit 1
ENTRYPOINT ["/app/quorum-api"]
CMD ["serve"]
