# Build stage
FROM rust:slim AS build
RUN apt-get update && apt-get install -y musl-tools pkg-config && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /usr/src/renews
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl

# Final minimal image
FROM scratch
COPY --from=build /usr/src/renews/target/x86_64-unknown-linux-musl/release/renews /renews
ENTRYPOINT ["/renews"]
