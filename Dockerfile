FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/
COPY examples/ examples/
RUN cargo build --release --examples

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    procps \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/examples/show_threads /app/
COPY --from=builder /app/target/release/examples/hostile_saturation /app/
COPY --from=builder /app/target/release/examples/demotion_promotion /app/
COPY --from=builder /app/target/release/examples/dynamic_triage /app/
COPY --from=builder /app/target/release/examples/prove_all /app/

ENTRYPOINT ["/app/prove_all"]
