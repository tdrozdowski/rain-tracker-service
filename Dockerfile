# Build stage
FROM rust:1.85 as builder

WORKDIR /usr/src/app

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source and migrations
COPY src ./src
COPY migrations ./migrations

# Copy .sqlx directory for offline mode (must exist)
COPY .sqlx ./.sqlx

# Set SQLx to offline mode
ENV SQLX_OFFLINE=true

# Build application in release mode
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/rain-tracker-service /app/
COPY --from=builder /usr/src/app/migrations /app/migrations

EXPOSE 8080

CMD ["./rain-tracker-service"]
