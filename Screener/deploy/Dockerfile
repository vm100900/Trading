# Build context must be the workspace root (Screener/), because screener-service
# depends on the sibling crate screener-core:
#     docker build -f deploy/Dockerfile -t screener-service .
# (docker compose in this directory already sets `context: ..`)

FROM rust:1-bookworm AS build
WORKDIR /src

# Dependency layer: copy just the manifests so `cargo fetch` is cached until a
# Cargo.toml actually changes.
COPY Cargo.toml ./
COPY crates/screener-core/Cargo.toml crates/screener-core/Cargo.toml
COPY crates/screener-service/Cargo.toml crates/screener-service/Cargo.toml
RUN mkdir -p crates/screener-core/src crates/screener-service/src \
    && echo 'fn main() {}' > crates/screener-service/src/main.rs \
    && echo '' > crates/screener-core/src/lib.rs \
    && cargo fetch

# Real sources.
COPY crates/screener-core crates/screener-core
COPY crates/screener-service crates/screener-service
RUN cargo build --release -p screener-service \
    && strip target/release/screener-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/screener-service /usr/local/bin/screener-service

# App auth token, bind address, engine selection, IBKR address, universe file
# are all supplied at runtime via environment (see deploy/.env.example).
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/screener-service"]
