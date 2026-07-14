# ============================================================================
# Synabit Coordinator Server
# ============================================================================

# --- Stage 1: Build ---
FROM rust:latest AS builder

WORKDIR /build

COPY Cargo.toml ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release 2>/dev/null || true

COPY src/ src/
RUN touch src/main.rs && cargo build --release

# --- Stage 2: Runtime ---
FROM debian:trixie-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd -r synabit && useradd -r -g synabit -m synabit

WORKDIR /app
COPY --from=builder /build/target/release/coordinator-server /app/coordinator-server
RUN chown -R synabit:synabit /app

USER synabit

EXPOSE 8090/tcp

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8090/health || exit 1

ENTRYPOINT ["/app/coordinator-server"]
