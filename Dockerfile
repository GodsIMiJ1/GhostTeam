# syntax=docker/dockerfile:1

FROM rust:1.82-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY schema.sql ./
COPY .ghostteam ./.ghostteam

RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates sqlite3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ghostteam /usr/local/bin/ghostteam
COPY .ghostteam /.ghostteam

ENV GHOSTTEAM_WORKSPACE_DIR=/.ghostteam
ENV GHOSTTEAM_API_PORT=8080
ENV RUST_LOG=info

EXPOSE 8080

CMD ["ghostteam", "--api"]
