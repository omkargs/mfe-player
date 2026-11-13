FROM rust:1.85-slim AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY src ./src
COPY assets ./assets
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    mpv \
    yt-dlp \
    python3 \
    ffmpeg \
    cava \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mfe-player /usr/local/bin/mfe-player
RUN ln -s mfe-player /usr/local/bin/mfe

ENV HOME=/root
WORKDIR /root
CMD ["mfe-player"]
