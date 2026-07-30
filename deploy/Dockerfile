# ============================================
# Stage 1: Build
# ============================================
FROM rust:1.80-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY apps/ apps/

# Build release binary
RUN cargo build --release --bin business-api

# ============================================
# Stage 2: Runtime
# ============================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash appuser

WORKDIR /app

COPY --from=builder /app/target/release/business-api /app/business-api
COPY config/ /app/config/
COPY migrations/ /app/migrations/

USER appuser

EXPOSE 3000

ENV RUST_LOG=info
ENV APP_ENVIRONMENT=production

ENTRYPOINT ["/app/business-api"]
