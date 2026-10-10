FROM rust:1.98.0 AS builder

WORKDIR /usr/src/app

COPY ../Cargo.toml ../Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY ../src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*
    
COPY --from=builder /usr/src/app/target/release/cosy-gameapi /usr/local/bin/cosy-gameapi

CMD ["cosy-gameapi"]
