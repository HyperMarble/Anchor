FROM rust:1.89.0-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build --release && \
    rm -rf target/release/build target/release/deps target/release/incremental
