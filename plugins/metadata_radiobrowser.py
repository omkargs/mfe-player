#!/usr/bin/env python3
"""MFE Player metadata plugin: Radio Browser.

Queries the community-maintained Radio Browser directory (radio-browser.info)
and returns live stations as playable tracks, so a search can drop straight
into a live stream.

Protocol:
    main.py search <query>  -> unified artists/albums/tracks
    main.py artist <name>   -> empty (a station is not an artist page)
    main.py album  <id>     -> empty for the same reason
"""
import json
import sys
import urllib.parse
import urllib.request

UA = "mfe-player/0.2 (+https://github.com/omkargs/mfe-player)"
MIRRORS = (
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://all.api.radio-browser.info",
)


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get(path, params):
    query = urllib.parse.urlencode(params)
    last = None
    for mirror in MIRRORS:
        request = urllib.request.Request(
            f"{mirror}{path}?{query}", headers={"User-Agent": UA}
        )
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                return json.load(response)
        except Exception as exc:
            last = exc
    raise last or RuntimeError("no Radio Browser mirror answered")


def station_to_track(station):
    identifier = station.get("stationuuid")
    url_ok = station.get("url_resolved") or station.get("url")
    if not identifier or not url_ok:
        return None
    name = (station.get("name") or "").strip()
    if not name:
        return None
    tags = ", ".join(
        tag for tag in (station.get("tags") or "").split(",") if tag.strip()
    )[:60]
    country = (station.get("country") or "").strip()
    artist = " · ".join(part for part in (country, tags) if part) or "Live radio"
    return {
        "id": f"radio:{identifier}",
        "title": name,
        "artist": artist,
        "album": "live radio",
        "duration": None,
        "artwork": station.get("favicon") or None,
        "source": "radio-browser",
        "year": None,
    }


def cmd_search(query):
    stations = get(
        "/json/stations/search",
        {
            "name": query,
            "limit": "20",
            "hidebroken": "true",
            "order": "votes",
            "reverse": "true",
        },
    )
    tracks = []
    for station in stations:
        track = station_to_track(station)
        if track:
            tracks.append(track)
    return {"artists": [], "albums": [], "tracks": tracks}


def main():
    if len(sys.argv) < 3:
        fail("usage: main.py search '<query>'")
    if sys.argv[1] == "search":
        print(json.dumps(cmd_search(sys.argv[2])))
    else:
        print(json.dumps({"artists": [], "albums": [], "tracks": []}))


if __name__ == "__main__":
    main()