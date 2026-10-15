# ── Build stage ─────────────────────────────────────────────────────
FROM rust:1.98-bookworm AS builder
WORKDIR /app

# The `sqlx::query!` macros verify their SQL at compile time. `.sqlx/` carries
# that metadata, so this image builds with no database anywhere: no build arg
# pointing at the developer's Postgres, and no reachability through the bridge
# gateway. Regenerate the cache with `make sqlx-prepare` after changing a query;
# CI fails if it is stale, so this stage cannot build against old metadata.
#
# It is copied after the dependency-only build, not before it: a layer is
# invalidated by anything above it, so putting an often-regenerated directory
# before that step would rebuild every dependency whenever a query changes.
ENV SQLX_OFFLINE=true

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release --locked && rm -rf src

COPY .sqlx ./.sqlx

COPY src ./src
COPY migrations ./migrations
COPY build.rs ./
RUN touch src/main.rs && cargo build --release --locked

# ── Runtime stage ───────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/tempox /app/server
COPY --from=builder /app/migrations /app/migrations

ENV BACKEND_PORT=3001
EXPOSE 3001
CMD ["/app/server"]
