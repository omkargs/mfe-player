#!/usr/bin/env python3
"""Nuclear TUI streaming plugin: SoundCloud via yt-dlp.

Returns the direct media URL plus a backup and the HTTP headers negotiated by
yt-dlp, with a small ranged probe to reject dead URLs before handing them to
the player.

Protocol:
    main.py resolve '<track-json>'
"""
import json
import shutil
import subprocess
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor

PROBE_BYTES = 2048
MAX_CANDIDATES = 4
PROBE_TIMEOUT = 4


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def format_headers(fmt, entry):
    headers = {}
    for source in (entry.get("http_headers") or {}, fmt.get("http_headers") or {}):
        for key, value in source.items():
            if not value:
                continue
            if key.lower() in ("accept-encoding", "host", "content-length", "range"):
                continue
            headers[key] = str(value)
    headers.setdefault("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
    return headers


def score(fmt):
    if fmt.get("vcodec") not in (None, "none"):
        return -1
    protocol = fmt.get("protocol") or ""
    quality = fmt.get("abr") or fmt.get("tbr") or 0
    bonus = 500 if protocol.startswith("http") and "m3u8" not in protocol else 0
    return quality + bonus


def candidates(entry):
    out, seen = [], set()
    formats = [fmt for fmt in (entry.get("formats") or []) if fmt.get("url")]
    formats.sort(key=score, reverse=True)
    for fmt in formats:
        if fmt.get("vcodec") not in (None, "none"):
            continue
        if fmt.get("acodec") in (None, "none"):
            continue
        url = fmt.get("url")
        if url and url not in seen:
            seen.add(url)
            out.append((url, format_headers(fmt, entry)))
        if len(out) >= MAX_CANDIDATES:
            break
    direct = entry.get("url")
    if direct and direct not in seen:
        out.append((direct, format_headers({}, entry)))
    return out


def probe(url, headers):
    request = urllib.request.Request(url, headers={**headers, "Range": f"bytes=0-{PROBE_BYTES - 1}"})
    try:
        with urllib.request.urlopen(request, timeout=PROBE_TIMEOUT) as response:
            if response.status not in (200, 206):
                return False
            response.read(256)
            return True
    except Exception:
        return False


def looks_like_preview(entry, url):
    """SoundCloud only hands out ~30s preview clips for some tracks."""
    if entry.get("is_preview"):
        return True
    lowered = url.lower()
    return "preview-media" in lowered or "media.sndcdn.com/preview" in lowered


def pick_candidates(options):
    if not options:
        return []
    with ThreadPoolExecutor(max_workers=min(4, len(options))) as pool:
        results = list(pool.map(lambda pair: probe(*pair), options))
    passed = [option for option, ok in zip(options, results) if ok]
    return passed or options


def main():
    if len(sys.argv) < 3 or sys.argv[1] != "resolve":
        fail("usage: main.py resolve '<track-json>'")
    try:
        track = json.loads(sys.argv[2])
    except json.JSONDecodeError as exc:
        fail(f"invalid track JSON: {exc}")

    title = str(track.get("title") or "").strip()
    artist = str(track.get("artist") or "").strip()
    if not title:
        fail("track has no title")
    query = f"{artist} - {title}" if artist else title

    if not shutil.which("yt-dlp"):
        fail("yt-dlp is not installed (needed by this streaming plugin)")

    command = [
        "yt-dlp",
        "--no-playlist",
        "--no-warnings",
        "--quiet",
        "--no-progress",
        "--retries",
        "3",
        "-f",
        "bestaudio/best",
        "-J",
        f"scsearch1:{query}",
    ]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=55)
    except subprocess.TimeoutExpired:
        fail("yt-dlp timed out while resolving the stream")
    if result.returncode != 0:
        detail = (result.stderr or "").strip().splitlines()
        fail(detail[-1] if detail else "yt-dlp failed to resolve the stream")

    try:
        info = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        fail(f"could not parse yt-dlp output: {exc}")

    entries = info.get("entries") or [info]
    entry = next((item for item in entries if item), None)
    if not entry:
        fail("no matching SoundCloud stream was found")

    options = candidates(entry)
    if not options and entry.get("webpage_url"):
        fallback = subprocess.run(
            [
                "yt-dlp",
                "--no-playlist",
                "--no-warnings",
                "--quiet",
                "--no-progress",
                "-f",
                "bestaudio/best",
                "-g",
                entry["webpage_url"],
            ],
            capture_output=True,
            text=True,
            timeout=55,
        )
        if fallback.returncode == 0:
            lines = [line.strip() for line in fallback.stdout.splitlines() if line.strip()]
            if lines:
                options = [(lines[-1], format_headers({}, entry))]

    if not options:
        fail("yt-dlp did not return a playable media URL")

    chosen = pick_candidates(options)
    full = [option for option in chosen if not looks_like_preview(entry, option[0])]
    short_preview = not full
    if full:
        # Full streams first, but keep clones as backups below them.
        chosen = full + [option for option in chosen if option not in full]
    primary = chosen[0]
    backup = chosen[1] if len(chosen) > 1 else None

    duration = entry.get("duration")
    if isinstance(duration, float):
        duration = int(duration)
    elif not isinstance(duration, int):
        duration = None

    print(
        json.dumps(
            {
                "url": primary[0],
                "backup_url": backup[0] if backup else None,
                "webpage_url": entry.get("webpage_url"),
                "http_headers": primary[1],
                "title": entry.get("title"),
                "duration": duration,
                "short_preview": short_preview,
                "extractor": entry.get("extractor_key") or entry.get("extractor") or "Soundcloud",
            }
        )
    )


if __name__ == "__main__":
    main()
