#!/usr/bin/env python3
"""MFE Player streaming plugin: YouTube via yt-dlp.

Resolves a track to one or more direct audio URLs. The URL is validated with a
tiny ranged request and comes with the HTTP headers yt-dlp negotiated, so mpv
does not get random 403s. A backup URL and the webpage URL are also returned:
the player can fall back to them if a Google video host rejects the first one.

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


def ytdlp(args, timeout=55):
    return subprocess.run(
        [
            "yt-dlp",
            "--no-playlist",
            "--no-warnings",
            "--quiet",
            "--no-progress",
            "--retries",
            "3",
            "--fragment-retries",
            "3",
            *args,
        ],
        capture_output=True,
        text=True,
        timeout=timeout,
    )


def extract(query, extra_args):
    target = query if query.startswith(("http://", "https://")) else f"ytsearch1:{query}"
    result = ytdlp(
        ["-f", "bestaudio/best", "--extractor-args", *extra_args, "-J", target]
    )
    if result.returncode != 0:
        return None
    try:
        info = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    entries = info.get("entries") or [info]
    return next((item for item in entries if item), None)


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
    out = []
    seen = set()
    formats = [fmt for fmt in (entry.get("formats") or []) if fmt.get("url")]
    formats.sort(key=score, reverse=True)
    for fmt in formats:
        audio_only = fmt.get("vcodec") in (None, "none")
        if not audio_only or fmt.get("acodec") in (None, "none"):
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
    """Check the first bytes are actually fetchable. Returns True/False."""
    request = urllib.request.Request(url, headers={**headers, "Range": f"bytes=0-{PROBE_BYTES - 1}"})
    try:
        with urllib.request.urlopen(request, timeout=PROBE_TIMEOUT) as response:
            if response.status not in (200, 206):
                return False
            response.read(256)
            return True
    except Exception:
        return False


def pick_candidates(entry):
    options = candidates(entry)
    # Probe every option at once instead of one after another: resolving a track
    # should not wait four timeouts before falling back to the unprobed list.
    if options:
        with ThreadPoolExecutor(max_workers=min(4, len(options))) as pool:
            results = list(pool.map(lambda pair: probe(*pair), options))
        passed = [option for option, ok in zip(options, results) if ok]
        return passed or options
    return []


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

    # Tracks imported from a link carry their page URL in the id. Resolve that
    # URL directly instead of searching for the title again.
    track_id = str(track.get("id") or "").strip()
    target = ""
    if track_id.startswith("url:"):
        target = track_id[4:]
    elif track_id.startswith(("http://", "https://")):
        target = track_id
    if not target:
        target = str(track.get("webpage_url") or "").strip()

    if not shutil.which("yt-dlp"):
        fail("yt-dlp is not installed (needed by this streaming plugin)")

    direct_extensions = (".mp3", ".m4a", ".aac", ".opus", ".ogg", ".flac", ".wav")
    if target and target.split("?")[0].lower().endswith(direct_extensions):
        duration = track.get("duration")
        if not isinstance(duration, int):
            duration = None
        print(
            json.dumps(
                {
                    "url": target,
                    "title": title,
                    "duration": duration,
                    "extractor": "Direct link",
                }
            )
        )
        return

    search = target or query
    try:
        entry = extract(search, ["youtube:player_client=default,web_safari"])
        if entry:
            chosen = pick_candidates(entry)
        else:
            chosen = None
        if not chosen:
            entry = extract(search, ["youtube:player_client=android_vr,mweb,tv"])
            chosen = pick_candidates(entry) if entry else None
    except subprocess.TimeoutExpired:
        fail("yt-dlp timed out while resolving the stream")

    if not entry or not chosen:
        fail("no playable YouTube audio source was found")

    duration = entry.get("duration")
    if isinstance(duration, float):
        duration = int(duration)
    elif not isinstance(duration, int):
        duration = None

    primary = chosen[0]
    backup = chosen[1] if len(chosen) > 1 else None
    payload = {
        "url": primary[0],
        "backup_url": backup[0] if backup else None,
        "webpage_url": entry.get("webpage_url"),
        "http_headers": primary[1],
        "title": entry.get("title"),
        "duration": duration,
        "extractor": entry.get("extractor_key") or entry.get("extractor") or "Youtube",
    }
    print(json.dumps(payload))


if __name__ == "__main__":
    main()
