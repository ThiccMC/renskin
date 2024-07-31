FROM rust:1-slim AS builder
WORKDIR /app

COPY . .
RUN cargo install --path .

# Stage 2: Create the final image
FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /usr/local/cargo/bin/rskd /app/rskd

# Copy cleaning script
COPY ./scripts/gc.sh ./
RUN chmod +x /app/gc.sh

# Install cron and dependencies
RUN apt-get update && \
    apt-get install -y cron && \
    rm -rf /var/lib/apt/lists/*

RUN echo "0 * * * * /usr/bin/sh /app/gc.sh" > /etc/cron.d/clean_cache

# Set permissions for crontab
RUN chmod 644 /etc/cron.d/clean_cache

# Expose port (if necessary)
EXPOSE 3727

# Command to run the application
CMD ["/app/rskd"]