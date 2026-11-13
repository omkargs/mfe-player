# syntax=docker/dockerfile:1

# ---- build ------------------------------------------------------------------
FROM rust:1.85-slim AS build

WORKDIR /src

# Cache the dependency graph separately from the source: a rebuild after a
# code-only change then takes seconds instead of minutes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
COPY assets ./assets
# Bundled provider scripts are embedded with include_str! at compile time.
COPY plugins ./plugins
RUN touch src/main.rs && cargo build --release --locked

# ---- runtime ----------------------------------------------------------------
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
    HOME=/root \
    XDG_CONFIG_HOME=/root/.config \
    XDG_CACHE_HOME=/root/.cache \
    TERM=xterm-256color

# The same runtime the native install needs: playback, search, metadata,
# artwork and the spectrum visualiser.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        mpv \
        yt-dlp \
        python3 \
        ffmpeg \
        cava \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/mfe-player /usr/local/bin/mfe-player
RUN ln -sf /usr/local/bin/mfe-player /usr/local/bin/mfe

WORKDIR /root
ENTRYPOINT ["mfe-player"]
