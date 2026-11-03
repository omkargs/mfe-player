#!/usr/bin/env bash
# MFE Player installer — build from source and put it on your PATH.
set -euo pipefail

say() { printf '\033[1;35m▸\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || die "Rust is missing. Install it from https://rustup.rs"
command -v mpv >/dev/null || die "mpv is missing. Install it with your package manager."
command -v python3 >/dev/null || die "python3 is missing."
command -v yt-dlp >/dev/null || say "warning: yt-dlp not found — streaming plugins need it"

say "building release binary (this takes a few minutes)"
cargo build --release

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"
install -m755 target/release/mfe-player "$BIN_DIR/mfe-player"
ln -sf "$BIN_DIR/mfe-player" "$BIN_DIR/mfe"

say "installed $BIN_DIR/mfe-player"
say "run: mfe"
