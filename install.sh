#!/usr/bin/env bash
#
# MFE Player installer.
#
#   curl -fsSL https://raw.githubusercontent.com/omkargs/mfe-player/master/install.sh | bash
#
# Run it and it installs the player plus everything it needs to play music:
# mpv, python3, yt-dlp, ffmpeg, cava and playerctl. Missing pieces are installed
# with your package manager. No prompts, no flags.
set -euo pipefail

REPO="omkargs/mfe-player"
NAME="mfe-player"
PREFIX="${PREFIX:-$HOME/.local/bin}"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/mfe-player"
SOURCE_DIR="${MFE_SOURCE_DIR:-$CACHE_DIR/src}"

# ---------------------------------------------------------------------------
# output
# ---------------------------------------------------------------------------

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != "dumb" ]; then
    C_RESET=$'\033[0m'
    C_BOLD=$'\033[1m'
    C_DIM=$'\033[2m'
    C_ACCENT=$'\033[38;2;45;212;191m'
    C_ACCENT2=$'\033[38;2;167;139;250m'
    C_GREEN=$'\033[38;2;126;231;135m'
    C_YELLOW=$'\033[38;2;255;209;102m'
    C_RED=$'\033[38;2;255;110;110m'
else
    C_RESET="" C_BOLD="" C_DIM="" C_ACCENT="" C_ACCENT2=""
    C_GREEN="" C_YELLOW="" C_RED=""
fi

say()  { printf '  %s▸%s %s\n' "$C_ACCENT" "$C_RESET" "$*"; }
ok()   { printf '  %s✓%s %s\n' "$C_GREEN" "$C_RESET" "$*"; }
skip() { printf '  %s·%s %s\n' "$C_DIM" "$C_RESET" "$*"; }
warn() { printf '  %s!%s %s\n' "$C_YELLOW" "$C_RESET" "$*"; }
die()  { printf '\n  %s✗ %s%s\n\n' "$C_RED" "$*" "$C_RESET" >&2; exit 1; }
step() {
    printf '\n  %s%s%s\n' "$C_BOLD" "$*" "$C_RESET"
    printf '  %s%s%s\n' "$C_DIM" "────────────────────────────────────────────────────────" "$C_RESET"
}

have() { command -v "$1" >/dev/null 2>&1; }
dry()  { printf '  %s~%s would %s\n' "$C_YELLOW" "$C_RESET" "$*"; }

# Arrow-key chooser used on Windows shells: Docker (recommended) or the
# experimental native install. Falls back silently when stdin is not a tty.
choose_windows_mode() {
    WINDOWS_MODE="native"
    [ -t 0 ] || { warn "not interactive - continuing with the native install"; return 0; }
    if ! have docker; then
        warn "docker not found - continuing with the native install"
        return 0
    fi
    printf '  %sHow do you want to run MFE?%s %s(↑/↓ + Enter)%s\n' "$C_BOLD" "$C_RESET" "$C_DIM" "$C_RESET"
    local selected=0
    local label0="Docker   - recommended: stable, runs in a container"
    local label1="Native   - experimental: some features may not work"
    local draw
    draw() {
        printf '  %s❯ %s%s\033[K\n' "$C_ACCENT" "$C_RESET" "$label0"
        printf '    %s%s\033[K\n' "$C_DIM" "$label1"
        printf '\033[2A'
    }
    draw
    while true; do
        local key
        IFS= read -rsn1 key
        case "$key" in
            $'\x1b')
                IFS= read -rsn2 key
                case "$key" in
                    '[A'|'D') selected=0 ;;
                    '[B'|'C') selected=1 ;;
                esac
                ;;
            '') break ;;
            'j'|'[B') selected=1 ;;
            'k'|'[A') selected=0 ;;
            '1') selected=0; break ;;
            '2') selected=1; break ;;
            'q') printf '\n'; die "cancelled" ;;
        esac
        draw
    done
    printf '\033[2B\033[K\033[B\033[K'
    if [ "$selected" = 0 ]; then
        WINDOWS_MODE="docker"
        ok "Docker selected - the stable way to run MFE on Windows"
    else
        WINDOWS_MODE="native"
        warn "Native selected - remember: experimental, some features may not work"
    fi
}


# ---------------------------------------------------------------------------
# logo
# ---------------------------------------------------------------------------

LOGO_ART='███╗   ███╗███████╗███████╗
████╗ ████║██╔════╝██╔════╝
██╔████╔██║█████╗  █████╗
██║╚██╔╝██║██╔══╝  ██╔══╝
██║ ╚═╝ ██║██║     ███████╗
╚═╝     ╚═╝╚═╝     ╚══════╝
██████╗ ██╗      █████╗ ██╗   ██╗███████╗██████╗
██╔══██╗██║     ██╔══██╗╚██╗ ██╔╝██╔════╝██╔══██╗
██████╔╝██║     ███████║ ╚████╔╝ █████╗  ██████╔╝
██╔═══╝ ██║     ██╔══██║  ╚██╔╝  ██╔══╝  ██╔══██╗
██║     ███████╗██║  ██║   ██║   ███████╗██║  ██║
╚═╝     ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝'

# One frame: a bright edge wipes the wordmark in, then the gradient drifts and
# a soft specular band rolls across it.
LOGO_AWK='
function clamp(x, lo, hi) { return x < lo ? lo : (x > hi ? hi : x) }
function mix(a, b, t) { return a + (b - a) * t }
function colour(t,   i, fr, r, g, b) {
  t = t % 1; if (t < 0) t += 1
  i = int(t * 4); fr = t * 4 - i
  if (i == 0)      { r = mix(45, 96, fr);   g = mix(212, 165, fr); b = mix(191, 250, fr) }
  else if (i == 1) { r = mix(96, 167, fr);  g = mix(165, 139, fr); b = mix(250, 250, fr) }
  else if (i == 2) { r = mix(167, 244, fr); g = mix(139, 114, fr); b = mix(250, 182, fr) }
  else             { r = mix(244, 45, fr);  g = mix(114, 212, fr); b = mix(182, 191, fr) }
  return sprintf("%.0f;%.0f;%.0f", clamp(r, 0, 255), clamp(g, 0, 255), clamp(b, 0, 255))
}
{
  n = NR; line[n] = $0
  if (length($0) > W) W = length($0)
}
END {
  rows = n
  reveal = f * 2.25 - 2
  phase  = f * 0.024
  shine  = -8 + (f * 2.4) % (W + 16)
  breath = 0.90 + 0.10 * sin(f * 0.20)

  if (f > 0) printf "\033[%dA", rows
  for (r = 1; r <= rows; r++) {
    src = line[r]; out = ""
    for (c = 1; c <= W; c++) {
      ch = substr(src, c, 1)
      if (ch == "" || ch == " ") { out = out " "; continue }
      if (c > reveal)           { out = out " "; continue }
      rgb = colour((c / W) * 0.70 - phase + r * 0.012)
      split(rgb, p, ";")
      rr = p[1]; gg = p[2]; bb = p[3]
      band = exp(-0.5 * ((c - shine) / 2.4) ^ 2) * 0.50
      if (reveal < W + 1 && c <= reveal + 3 && band < 0.85) band = 0.85
      rr = rr + (255 - rr) * band
      gg = gg + (255 - gg) * band
      bb = bb + (255 - bb) * band
      out = out sprintf("\033[38;2;%.0f;%.0f;%.0fm%s", rr * breath, gg * breath, bb * breath, ch)
    }
    printf "  %s\033[0m\033[K\n", out
  }
}'

logo_plain() {
    printf '\n'
    printf '%s\n' "$LOGO_ART" | while IFS= read -r line; do
        printf '  %s%s%s\n' "$C_ACCENT" "$line" "$C_RESET"
    done
    printf '\n  %s%s%s\n\n' "$C_DIM" "a terminal music player · no account · no subscription" "$C_RESET"
}

logo_animate() {
    local f
    printf '\033[?25l'
    for ((f = 0; f < 52; f++)); do
        printf '%s\n' "$LOGO_ART" | awk -v f="$f" "$LOGO_AWK"
        sleep 0.04
    done
    printf '\033[?25h'
    printf '  %s%s%s\n\n' "$C_DIM" "a terminal music player · no account · no subscription" "$C_RESET"
}

show_logo() {
    if [ ! -t 1 ] || [ -n "${NO_COLOR:-}" ]; then
        logo_plain
        return
    fi
    if [ "$(tput cols 2>/dev/null || echo 80)" -lt 58 ]; then
        logo_plain
        return
    fi
    case "${COLORTERM:-}" in
        *truecolor*|*24bit*|*24BIT*) ;;
        *) logo_plain; return ;;
    esac
    if ! sleep 0.01 2>/dev/null; then
        logo_plain
        return
    fi
    logo_animate
}

# ---------------------------------------------------------------------------
# platform
# ---------------------------------------------------------------------------

OS="$(uname -s)"
ARCH="$(uname -m)"
PKG_MANAGER=""
SUDO=""

detect_manager() {
    if [ "$OS" = "Darwin" ]; then
        have brew && PKG_MANAGER="brew"
        return
    fi
    local candidate
    for candidate in pacman apt-get dnf zypper apk xbps-install emerge; do
        if have "$candidate"; then
            PKG_MANAGER="$candidate"
            return
        fi
    done
}

if [ "$(id -u)" != "0" ] && have sudo; then
    SUDO="sudo"
fi

pkg_name() {
    case "$PKG_MANAGER:$1" in
        pacman:python3) echo "python" ;;
        brew:python3) echo "python@3.12" ;;
        *) echo "$1" ;;
    esac
}

pkg_install() {
    local packages=("$@")
    [ ${#packages[@]} -eq 0 ] && return 0

    local mapped=() package
    for package in "${packages[@]}"; do
        mapped+=("$(pkg_name "$package")")
    done

    case "$PKG_MANAGER" in
        pacman)       $SUDO pacman -S --needed --noconfirm "${mapped[@]}" ;;
        apt-get)
            $SUDO apt-get update -qq
            DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y "${mapped[@]}"
            ;;
        dnf)          $SUDO dnf install -y "${mapped[@]}" ;;
        zypper)       $SUDO zypper --non-interactive install "${mapped[@]}" ;;
        apk)          $SUDO apk add "${mapped[@]}" ;;
        xbps-install) $SUDO xbps-install -y "${mapped[@]}" ;;
        emerge)       $SUDO emerge --ask=n "${mapped[@]}" ;;
        brew)         brew install "${mapped[@]}" ;;
        *) return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# dependencies
# ---------------------------------------------------------------------------

MISSING_REQUIRED=()

install_dependency() {
    local tool="$1" purpose="$2" required="$3"

    if have "$tool"; then
        ok "$tool"
        return 0
    fi

    if [ "$DRY_RUN" = 1 ]; then
        dry "install $tool ($purpose)"
        return 0
    fi

    if [ -n "$PKG_MANAGER" ]; then
        say "installing $tool ($purpose)"
        if pkg_install "$tool"; then
            ok "$tool"
            return 0
        fi
    fi

    # yt-dlp ships slowly in distro repos; pipx/pip are the standard fallback.
    if [ "$tool" = "yt-dlp" ]; then
        if have pipx && pipx install yt-dlp >/dev/null 2>&1; then
            ok "yt-dlp (pipx)"
            return 0
        fi
        if have pip3 && pip3 install --user --upgrade yt-dlp >/dev/null 2>&1; then
            ok "yt-dlp (pip3)"
            return 0
        fi
    fi

    warn "$tool is missing — $purpose will be unavailable"
    [ "$required" = 1 ] && MISSING_REQUIRED+=("$tool")
    return 1
}

check_dependencies() {
    step "Dependencies"
    detect_manager
    [ -n "$PKG_MANAGER" ] && skip "package manager: $PKG_MANAGER"

    install_dependency mpv "audio playback" 1
    if have python3 || have python; then
        ok "python3"
    else
        install_dependency python3 "plugins" 1
    fi
    install_dependency yt-dlp "streaming" 1
    install_dependency ffmpeg "downloads" 0
    install_dependency cava "spectrum visualiser" 0
    install_dependency playerctl "media keys" 0

    if [ ${#MISSING_REQUIRED[@]} -gt 0 ]; then
        die "missing required dependencies: ${MISSING_REQUIRED[*]}"
    fi
}

# ---------------------------------------------------------------------------
# install mfe
# ---------------------------------------------------------------------------

target_triple() {
    case "$OS:$ARCH" in
        Linux:x86_64)
            if ldd --version 2>&1 | grep -qi musl; then
                echo "x86_64-unknown-linux-musl"
            else
                echo "x86_64-unknown-linux-gnu"
            fi
            ;;
        Linux:aarch64|Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
        Darwin:arm64) echo "aarch64-apple-darwin" ;;
        Darwin:x86_64) echo "x86_64-apple-darwin" ;;
        *) echo "" ;;
    esac
}

download() {
    local url="$1" dest="$2"
    if have curl; then
        curl -fsSL --retry 2 --connect-timeout 10 -o "$dest" "$url"
    elif have wget; then
        wget -q -O "$dest" "$url"
    else
        return 1
    fi
}

install_binary() {
    local source="$1"
    [ -f "$source" ] || die "binary not found: $source"
    if [ "$DRY_RUN" = 1 ]; then
        dry "install $source → $PREFIX/$NAME"
        return 0
    fi
    say "installing to $PREFIX"
    if [ -d "$PREFIX" ] && [ ! -w "$PREFIX" ] && [ -n "$SUDO" ]; then
        $SUDO mkdir -p "$PREFIX"
        $SUDO install -m 755 "$source" "$PREFIX/$NAME"
        $SUDO ln -sf "$NAME" "$PREFIX/mfe"
    else
        mkdir -p "$PREFIX"
        install -m 755 "$source" "$PREFIX/$NAME"
        ln -sf "$NAME" "$PREFIX/mfe"
    fi
    ok "$PREFIX/mfe"
}

install_from_release() {
    local triple url tmp archive bin
    triple="$(target_triple)"
    [ -z "$triple" ] && return 1
    if [ "$DRY_RUN" = 1 ]; then
        dry "download the prebuilt $triple release (fallback: clone into $SOURCE_DIR and build)"
        return 0
    fi

    url="https://github.com/$REPO/releases/latest/download/$NAME-$triple.tar.gz"
    tmp="$(mktemp -d)"
    archive="$tmp/$NAME.tar.gz"
    if ! download "$url" "$archive" 2>/dev/null; then
        rm -rf "$tmp"
        return 1
    fi
    if ! tar -xzf "$archive" -C "$tmp" 2>/dev/null; then
        rm -rf "$tmp"
        return 1
    fi
    bin="$(find "$tmp" -type f \( -name "$NAME" -o -name "mfe" \) -print -quit)"
    if [ -z "$bin" ]; then
        rm -rf "$tmp"
        return 1
    fi
    install_binary "$bin"
    rm -rf "$tmp"
    ok "installed the prebuilt release"
}

install_from_source() {
    local dir="$1"
    have cargo || return 1
    if [ "$DRY_RUN" = 1 ]; then
        dry "clone/build in ${dir:-$SOURCE_DIR}"
        return 0
    fi

    # Running from a checkout builds in place. Running from `curl | bash`
    # (or anywhere else) clones into a persistent cache, so the next build
    # is incremental instead of another throwaway /tmp/tmp.XXXX directory.
    if [ -z "$dir" ] || [ ! -f "$dir/Cargo.toml" ]; then
        have git || return 1
        say "fetching the source into $SOURCE_DIR"
        mkdir -p "$(dirname "$SOURCE_DIR")"
        if [ -d "$SOURCE_DIR/.git" ]; then
            if ! git -C "$SOURCE_DIR" fetch --depth 1 origin master >/dev/null 2>&1 \
                || ! git -C "$SOURCE_DIR" reset --hard FETCH_HEAD >/dev/null 2>&1; then
                return 1
            fi
        elif ! git clone --depth 1 "https://github.com/$REPO.git" "$SOURCE_DIR" >/dev/null 2>&1; then
            return 1
        fi
        dir="$SOURCE_DIR"
    fi

    say "building in $dir (a few minutes)"
    if ! ( cd "$dir" && cargo build --release --locked ); then
        return 1
    fi
    if [ ! -f "$dir/target/release/$NAME" ]; then
        warn "the build reported success but produced no binary (was $dir removed mid-build?)"
        return 1
    fi
    install_binary "$dir/target/release/$NAME"
    ok "built and installed from source (kept at $dir)"
}

install_from_npm() {
    have npm || return 1
    say "installing from npm"
    npm install -g "$NAME" >/dev/null 2>&1 || return 1
    local bin_dir
    bin_dir="$(npm prefix -g 2>/dev/null)/bin"
    if [ ! -x "$bin_dir/mfe" ] && [ ! -x "$bin_dir/$NAME" ]; then
        return 1
    fi
    ok "installed with npm ($bin_dir)"
}

install_with_docker() {
    step "Docker"
    if [ "$DRY_RUN" = 1 ]; then
        dry "clone the repo into $SOURCE_DIR and run docker compose"
        return 0
    fi
    have git || { warn "git is required for the docker install"; return 1; }
    say "fetching the compose files into $SOURCE_DIR"
    mkdir -p "$(dirname "$SOURCE_DIR")"
    if [ -d "$SOURCE_DIR/.git" ]; then
        git -C "$SOURCE_DIR" fetch --depth 1 origin master >/dev/null 2>&1 &&
            git -C "$SOURCE_DIR" reset --hard FETCH_HEAD >/dev/null 2>&1
    else
        git clone --depth 1 "https://github.com/$REPO.git" "$SOURCE_DIR" >/dev/null 2>&1
    fi || { warn "could not fetch the repository"; return 1; }
    say "building the image (a few minutes the first time)"
    ( cd "$SOURCE_DIR" && docker compose build ) || { warn "docker compose build failed"; return 1; }
    ok "image built and cached"
    printf '\n  %srun it with:%s\n' "$C_BOLD" "$C_RESET"
    printf '    %scd %s && docker compose run --rm mfe%s\n' "$C_ACCENT" "$SOURCE_DIR" "$C_RESET"
    return 0
}

install_mfe() {
    step "mfe-player"
    local dir="${SCRIPT_DIR:-}"

    if [ "$FROM_SOURCE" = 0 ] && install_from_release; then
        return 0
    fi
    if install_from_source "$dir"; then
        return 0
    fi
    if [ "$FROM_SOURCE" = 0 ] && install_from_npm; then
        return 0
    fi
    die "could not install mfe-player (release download, source build and npm all failed — see above)"
}

# ---------------------------------------------------------------------------
# extras
# ---------------------------------------------------------------------------

install_completions() {
    step "Completions"
    local bin="${MFE_BIN:-$PREFIX/$NAME}"
    [ -x "$bin" ] || { skip "binary is not installed"; return 0; }

    # npm can carry an older platform binary than this installer (0.2.0 does
    # not know `completions`; calling it would start the TUI and block).
    if ! "$bin" --help 2>/dev/null | grep -qi "completions"; then
        skip "this binary has no completions subcommand"
        return 0
    fi

    if [ "$DRY_RUN" = 1 ]; then
        dry "write bash/fish/zsh completions"
        return 0
    fi

    local bash_dir="$HOME/.local/share/bash-completion/completions"
    local fish_dir="$HOME/.config/fish/completions"
    local zsh_dir="$HOME/.zfunc"

    if mkdir -p "$bash_dir" && "$bin" completions bash > "$bash_dir/mfe" 2>/dev/null; then
        ok "bash → $bash_dir/mfe"
    fi
    if have fish || [ -d "$HOME/.config/fish" ]; then
        mkdir -p "$fish_dir"
        "$bin" completions fish > "$fish_dir/mfe.fish" 2>/dev/null \
            && ok "fish → $fish_dir/mfe.fish"
    fi
    if have zsh || [ -d "$zsh_dir" ]; then
        mkdir -p "$zsh_dir"
        "$bin" completions zsh > "$zsh_dir/_mfe" 2>/dev/null \
            && ok "zsh → $zsh_dir/_mfe (add it to fpath)"
    fi
}

run_doctor() {
    local bin="${MFE_BIN:-$PREFIX/$NAME}"
    if [ "$DRY_RUN" = 1 ]; then
        step "Health check"
        dry "run mfe doctor"
        return 0
    fi
    [ -x "$bin" ] || return 0
    step "Health check"

    # Same guard as completions: an older binary treats `doctor` as a search
    # query and opens the TUI, which would hang the installer.
    if ! "$bin" --help 2>/dev/null | grep -qi "doctor"; then
        skip "this binary has no doctor subcommand"
        return 0
    fi
    if "$bin" doctor; then
        ok "everything the player needs is in place"
    else
        warn "some pieces are missing — see the x lines above"
    fi
}

finish() {
    printf '\n'
    if [ "$DRY_RUN" = 1 ]; then
        printf '  %s%sDry run complete.%s\n' "$C_BOLD" "$C_YELLOW" "$C_RESET"
    elif [ "$SETUP_ONLY" = 1 ]; then
        printf '  %s%sSetup complete.%s\n' "$C_BOLD" "$C_GREEN" "$C_RESET"
    else
        printf '  %s%sInstalled.%s\n' "$C_BOLD" "$C_GREEN" "$C_RESET"
    fi
    printf '\n'
    printf '  %srun%s  %smfe%s          %sstart the player%s\n' "$C_DIM" "$C_RESET" "$C_BOLD" "$C_RESET" "$C_DIM" "$C_RESET"
    printf '  %s     %smfe doctor%s    %scheck mpv, plugins and config%s\n' "$C_DIM" "$C_BOLD" "$C_RESET" "$C_DIM" "$C_RESET"
    printf '  %s     %smfe --help%s    %severy subcommand%s\n' "$C_DIM" "$C_BOLD" "$C_RESET" "$C_DIM" "$C_RESET"
    case ":$PATH:" in
        *":$PREFIX:"*) ;;
        *)
            printf '\n  %s!%s %s is not on your PATH. Add this to your shell profile:\n' \
                "$C_YELLOW" "$C_RESET" "$PREFIX"
            printf '      %sexport PATH="%s:$PATH"%s\n' "$C_DIM" "$PREFIX" "$C_RESET"
            ;;
    esac
    if [ "$DRY_RUN" = 1 ]; then
        printf '\n  %sDry run — nothing was installed or changed.%s\n' "$C_YELLOW" "$C_RESET"
    fi
    printf '\n'
}

# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

cleanup() { printf '\033[?25h'; }
trap cleanup EXIT INT TERM

if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    # curl | bash: there is no checkout, force the persistent clone path.
    SCRIPT_DIR=""
fi

DRY_RUN=0
DEPS_ONLY=0
SETUP_ONLY=0
FROM_SOURCE=0
for arg in "$@"; do
    case "$arg" in
        --dry-run|--check|--preview) DRY_RUN=1 ;;
        --deps-only) DEPS_ONLY=1 ;;
        --setup|--npm-setup) SETUP_ONLY=1 ;;
        --from-source|--source) FROM_SOURCE=1 ;;
        -h|--help)
            printf 'mfe-player installer\n\n'
            printf '  bash install.sh              install everything\n'
            printf '  bash install.sh --dry-run    show every phase, change nothing\n'
            printf '  bash install.sh --deps-only  install missing dependencies only\n'
            printf '  bash install.sh --setup      deps + completions + doctor (no binary)\n'
            printf '  bash install.sh --from-source  skip the release/npm lanes, build from git\n'
            exit 0
            ;;
        *) printf 'unknown option: %s (try --help)\n' "$arg" >&2; exit 2 ;;
    esac
done

show_logo

step "Environment"
skip "$OS $ARCH"

check_dependencies

if [ "$DEPS_ONLY" = 1 ]; then
    if [ "$DRY_RUN" = 1 ]; then
        printf '\n  %sDry run — nothing was installed or changed.%s\n\n' "$C_YELLOW" "$C_RESET"
    else
        printf '\n  %sDependencies ready.%s\n\n' "$C_GREEN" "$C_RESET"
    fi
    exit 0
fi

if [ "$SETUP_ONLY" = 1 ]; then
    install_completions
    run_doctor
    finish
    exit 0
fi

# Windows shells (Git Bash / MSYS): the native build is experimental, so warn
# and let the user pick the Docker route with arrows.
case "$OS" in
    MINGW*|MSYS*|CYGWIN*)
        step "Windows notice"
        warn "Windows is EXPERIMENTAL — some features may not work yet."
        say "audio, media keys and desktop integrations are best-effort."
        say "the Docker build is the stable path on Windows."
        choose_windows_mode
        if [ "${WINDOWS_MODE:-native}" = "docker" ]; then
            if install_with_docker; then
                finish
                exit 0
            fi
            die "docker install failed - fix docker, or re-run and choose Native"
        fi
        ;;
esac

install_mfe
install_completions
run_doctor
finish
