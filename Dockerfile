# ════════════════════════════════════════════════════════════
#  Stria Trace — Multi-stage build
# ════════════════════════════════════════════════════════════

# ── Stage 1: Builder ──────────────────────────────────────
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation by copying manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/trace-test/Cargo.toml crates/trace-test/
RUN mkdir -p src crates/trace-test/src && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'fn main() {}' > crates/trace-test/src/main.rs && \
    cargo build --release && \
    rm -rf src crates/trace-test/src

# Copy full source and build
COPY . .
RUN touch src/main.rs crates/trace-test/src/main.rs && \
    cargo build --release

# ── Stage 2: Runtime ────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 -s /sbin/nologin trace

WORKDIR /app

COPY --from=builder /app/target/release/stria-trace /usr/local/bin/stria-trace
COPY --from=builder /app/target/release/trace-test /usr/local/bin/trace-test
COPY --from=builder /app/ui/index.html ./ui/index.html

RUN chown -R trace:trace /app
USER trace

EXPOSE 8080

ENV TRACE_BIND_ADDRESS=0.0.0.0
ENV TRACE_PORT=8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["sh", "-c", "wget -qO- http://localhost:8080/health || exit 1"]

ENTRYPOINT ["/usr/local/bin/stria-trace"]
