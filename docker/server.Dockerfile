FROM rust:1.89-bookworm AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --release -p evohime-server

FROM debian:bookworm-slim
WORKDIR /workspace
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/evohime-server /usr/local/bin/evohime-server
CMD ["evohime-server"]

