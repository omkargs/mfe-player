#!/usr/bin/env python3
"""MFE Player lyrics plugin: LRCLIB with lyrics.ovh fallback.

Protocol:
    main.py fetch '<track-json>'
Outputs: {"source": "...", "plain": "...", "synced": "..."}
"""
import json
import sys
import urllib.error
import urllib.parse
import urllib.request

UA = "MFEPlayer/0.1 (https://github.com/omkargs/mfe-player)"


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get_json(url, timeout):
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def lrclib(track):
    params = {
        "artist_name": track.get("artist") or "",
        "track_name": track.get("title") or "",
    }
    if track.get("album"):
        params["album_name"] = track["album"]
    if track.get("duration"):
        params["duration"] = int(track["duration"])
    try:
        item = get_json(
            "https://lrclib.net/api/get?" + urllib.parse.urlencode(params), 6
        )
        if item and (item.get("syncedLyrics") or item.get("plainLyrics")):
            return item
    except Exception:
        pass

    query = f"{track.get('artist', '')} {track.get('title', '')}".strip()
    try:
        results = get_json(
            "https://lrclib.net/api/search?" + urllib.parse.urlencode({"q": query}), 6
        )
    except Exception:
        return None
    if not isinstance(results, list):
        return None
    for item in results:
        if item.get("syncedLyrics") or item.get("plainLyrics"):
            return item
    return None


def lyrics_ovh(track):
    artist = urllib.parse.quote(track.get("artist") or "", safe="")
    title = urllib.parse.quote(track.get("title") or "", safe="")
    try:
        data = get_json(f"https://api.lyrics.ovh/v1/{artist}/{title}", 10)
        text = (data.get("lyrics") or "").replace("\r\n", "\n").strip()
        if text:
            return text
    except Exception:
        return None
    return None


def main():
    if len(sys.argv) < 3 or sys.argv[1] != "fetch":
        fail("usage: main.py fetch '<track-json>'")
    try:
        track = json.loads(sys.argv[2])
    except json.JSONDecodeError as exc:
        fail(f"invalid track JSON: {exc}")

    item = lrclib(track)
    if item:
        print(
            json.dumps(
                {
                    "source": "lrclib",
                    "plain": item.get("plainLyrics"),
                    "synced": item.get("syncedLyrics"),
                },
                ensure_ascii=False,
            )
        )
        return

    plain = lyrics_ovh(track)
    if plain:
        print(
            json.dumps(
                {"source": "lyrics.ovh", "plain": plain, "synced": None},
                ensure_ascii=False,
            )
        )
        return

    fail("no lyrics found for this track")


if __name__ == "__main__":
    main()
