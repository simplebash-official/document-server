# syntax=docker/dockerfile:1

# ------------------------------------------------------------------------------
# Stage 1: Chef Base (pinned to tested local Rust version)
# ------------------------------------------------------------------------------
FROM rust:1.97.0-slim-bookworm AS chef

WORKDIR /app

# Install build dependencies and cargo-chef
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    ca-certificates \
    && cargo install cargo-chef --locked \
    && rm -rf /var/lib/apt/lists/*

# ------------------------------------------------------------------------------
# Stage 2: Planner (computes recipe.json for Cargo.lock dependencies)
# ------------------------------------------------------------------------------
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ------------------------------------------------------------------------------
# Stage 3: Builder (cooks dependencies and compiles release binary)
# ------------------------------------------------------------------------------
FROM chef AS builder

# Cook dependencies from recipe — cached across builds unless Cargo.lock/toml changes
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy application source and build release binary
COPY . .
RUN cargo build --release && \
    strip target/release/document_server

# ------------------------------------------------------------------------------
# Stage 4: Minimal Runtime Image
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runner

# Install runtime utilities (curl for healthcheck, ca-certificates for TLS)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root system user and group (UID/GID 10001)
RUN groupadd -g 10001 appuser && \
    useradd -u 10001 -g appuser -d /app -s /bin/false -M appuser

WORKDIR /app

# Prepare directories for binary, assets, and persistent SQLite storage
RUN mkdir -p /app/data /app/templates /app/fonts && \
    chown -R appuser:appuser /app

# Copy stripped binary from builder stage
COPY --from=builder --chown=appuser:appuser /app/target/release/document_server /app/document_server

# Copy templates and fonts assets
COPY --chown=appuser:appuser templates/ /app/templates/
COPY --chown=appuser:appuser fonts/ /app/fonts/

# Environment defaults
ENV PORT=8090 \
    DATABASE_URL=sqlite:///app/data/document_server.db \
    TEMPLATES_DIR=/app/templates \
    FONTS_DIR=/app/fonts \
    MAX_RENDER_BODY_BYTES=5242880 \
    APP_ENV=production \
    ALLOW_PRIVATE_IP_IMAGES=false \
    RUST_LOG=document_server=info,tower_http=info,info

USER appuser

EXPOSE 8090

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8090/api/health || exit 1

CMD ["/app/document_server"]
