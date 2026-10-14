# ── Build stage ─────────────────────────────────────────────────────
FROM rust:1.98-bookworm AS builder
WORKDIR /app

# Query checking happens at compile time against the live database: compose
# passes this build arg with build network=host so the builder reaches the
# developer's Postgres. Without it the macro verification fails loudly.
ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release --locked && rm -rf src

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
