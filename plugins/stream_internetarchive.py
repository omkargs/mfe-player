#!/usr/bin/env python3
"""MFE Player streaming plugin: Internet Archive.

Resolves the `ia:<identifier>/<file>` ids minted by the metadata plugin into
the public download URL behind them. If a plain track slips through (for
example after a radio pass), it falls back to searching the archive first.

Protocol:
    main.py resolve '<track-json>'
"""
import json
import sys
import urllib.parse
import urllib.request

UA = "mfe-player/0.2 (+https://github.com/omkargs/mfe-player)"
DOWNLOAD = "https://archive.org/download/{identifier}/{name}"
SEARCH = "https://archive.org/advancedsearch.php"
METADATA = "https://archive.org/metadata/{identifier}"


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def first_audio(identifier):
    data = get(METADATA.format(identifier=urllib.parse.quote(identifier)))
    for info in data.get("files") or []:
        fmt = str(info.get("format") or info.get("name") or "").lower()
        name = str(info.get("name") or "")
        if any(hint in fmt for hint in ("mp3", "flac", "ogg", "opus")) and name:
            return name
    return None


def search_for(track):
    query = " - ".join(
        part for part in (track.get("artist"), track.get("title")) if part
    )
    params = {
        "q": f"(mediatype:(audio)) AND (title:({query}) OR description:({query}))",
        "fl[]": ["identifier"],
        "rows": "3",
        "page": "1",
        "output": "json",
    }
    docs = (get(f"{SEARCH}?{urllib.parse.urlencode(params, doseq=True)}").get(
        "response", {}
    ).get("docs") or [])
    for doc in docs:
        identifier = doc.get("identifier")
        if not identifier:
            continue
        name = first_audio(identifier)
        if name:
            return identifier, name
    return None, None


def main():
    if len(sys.argv) < 3 or sys.argv[1] != "resolve":
        fail("usage: main.py resolve '<track-json>'")
    try:
        track = json.loads(sys.argv[2])
    except json.JSONDecodeError as exc:
        fail(f"invalid track JSON: {exc}")

    identifier = name = None
    track_id = str(track.get("id") or "")
    if track_id.startswith("ia:"):
        identifier, _, name = track_id[3:].partition("/")
    if not identifier or not name:
        try:
            identifier, name = search_for(track)
        except Exception as exc:
            fail(f"internet archive lookup failed: {exc}")
    if not identifier or not name:
        fail("no Internet Archive copy of that track was found")

    url = DOWNLOAD.format(
        identifier=urllib.parse.quote(identifier), name=urllib.parse.quote(name)
    )
    print(
        json.dumps(
            {
                "url": url,
                "webpage_url": f"https://archive.org/details/{identifier}",
                "title": track.get("title"),
                "duration": track.get("duration"),
                "extractor": "Internet Archive",
            }
        )
    )


if __name__ == "__main__":
    main()