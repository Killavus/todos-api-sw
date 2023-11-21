FROM rust:latest as builder

WORKDIR /usr/src/app
COPY . .
RUN apt-get update && apt-get install -y musl musl-tools
RUN rustup target add x86_64-unknown-linux-musl
# Will build and cache the binary and dependent crates in release mode
RUN cargo build --release --target x86_64-unknown-linux-musl && mv ./target/x86_64-unknown-linux-musl/release/todos-api ./todos-api

# Runtime image
FROM debian:bullseye-slim

# Run as "app" user
RUN useradd -ms /bin/bash app

USER app
WORKDIR /app

# Get compiled binaries from builder's cargo install directory
COPY --from=builder /usr/src/app/todos-api /app/todos-api

# Run the app
CMD ./todos-api
