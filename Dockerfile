# Build stage
FROM rust:1.85-bookworm as builder

# Build argument for version
ARG VERSION=dev

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Store version information
RUN echo "${VERSION}" > /app/target/release/VERSION

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 udp-director

WORKDIR /app

# Copy the binary and version from builder
COPY --from=builder /app/target/release/udp-director /app/udp-director
COPY --from=builder /app/target/release/VERSION /app/VERSION

# Change ownership
RUN chown -R udp-director:udp-director /app

USER udp-director

EXPOSE 9000/tcp
EXPOSE 7777/udp
EXPOSE 9090/tcp

ENTRYPOINT ["/app/udp-director"]
