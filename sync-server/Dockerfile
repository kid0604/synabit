# ============================================================================
# Synabit Sync Server — Multi-stage Docker build
#
# Stage 1: Build the Rust binary with musl for a fully static executable.
# Stage 2: Copy into a minimal distroless image for production.
# ============================================================================

# --- Stage 1: Build ---
FROM rust:latest AS builder

WORKDIR /build

# Cache dependency downloads by copying manifests first.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Now copy the real source and build.
COPY src/ src/
RUN touch src/main.rs && cargo build --release

# --- Stage 2: Runtime (must match builder's glibc) ---
FROM debian:trixie-slim AS runtime

# Install minimal runtime dependencies (SQLite is bundled, so only libc needed).
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Create a non-root user for the server process.
RUN groupadd -r synabit && useradd -r -g synabit -m synabit

WORKDIR /app

# Copy the built binary.
COPY --from=builder /build/target/release/synabit-sync-server /app/synabit-sync-server

# Copy default config.
COPY config.example.toml /app/config.toml

# Create data directory and give ownership to the non-root user.
RUN mkdir -p /app/data && chown -R synabit:synabit /app

USER synabit

# Expose ports:
#   4433 - QUIC (Iroh mailbox protocol)
#   8080 - HTTP health endpoint
EXPOSE 4433/udp
EXPOSE 8080/tcp

# Health check using the HTTP endpoint.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["/app/synabit-sync-server"]
CMD ["--config", "/app/config.toml", "--data-dir", "/app/data"]
