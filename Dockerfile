FROM rustlang/rust:nightly-bookworm-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rskd /usr/local/bin/rskd
ENV RENSKIN_BIND=0.0.0.0:3727 RENSKIN_CACHE_DIR=/tmp/renskin-cache
EXPOSE 3727
USER 65534:65534
CMD ["/usr/local/bin/rskd"]
