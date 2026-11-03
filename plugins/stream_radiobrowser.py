#!/usr/bin/env python3
"""MFE Player streaming plugin: Radio Browser.

Turns the `radio:<uuid>` ids minted by the metadata plugin into the live stream
URL behind them (mpv plays ICY/SHOUTcast natively, metadata and all).

Protocol:
    main.py resolve '<track-json>'
"""
import json
import sys
import urllib.parse
import urllib.request
from urllib.error import HTTPError, URLError

UA = "mfe-player/0.2 (+https://github.com/omkargs/mfe-player)"
MIRRORS = (
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://all.api.radio-browser.info",
)


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get(path):
    last = None
    for mirror in MIRRORS:
        request = urllib.request.Request(
            f"{mirror}{path}", headers={"User-Agent": UA}
        )
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                return json.load(response)
        except HTTPError as exc:
            if exc.code == 404:
                return None
            last = exc
        except Exception as exc:
            last = exc
    if last and not isinstance(last, HTTPError):
        raise last
    return None


def main():
    if len(sys.argv) < 3 or sys.argv[1] != "resolve":
        fail("usage: main.py resolve '<track-json>'")
    try:
        track = json.loads(sys.argv[2])
    except json.JSONDecodeError as exc:
        fail(f"invalid track JSON: {exc}")

    track_id = str(track.get("id") or "")
    station = None
    if track_id.startswith("radio:"):
        stations = get(f"/json/stations/byuuid/{track_id[6:]}")
        if isinstance(stations, list) and stations:
            station = stations[0]
    if station is None:
        query = urllib.parse.quote(str(track.get("title") or ""))
        if not query:
            fail("station id missing from track")
        stations = get(f"/json/stations/byname/{query}?limit=1")
        if isinstance(stations, list) and stations:
            station = stations[0]
    if station is None:
        fail("station not found in Radio Browser")

    url = station.get("url_resolved") or station.get("url")
    if not url:
        fail("station has no playable stream URL")

    print(
        json.dumps(
            {
                "url": url,
                "webpage_url": station.get("homepage") or None,
                "title": track.get("title"),
                "duration": None,
                "extractor": f"Radio · {station.get('name') or 'station'}",
            }
        )
    )


if __name__ == "__main__":
    main()
