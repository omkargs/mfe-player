#!/usr/bin/env python3
"""MFE Player metadata plugin: Internet Archive.

Searches eight hundred years of free music, live sets and public-domain
recordings and turns the audio files inside a collection into playable tracks.

Protocol:
    main.py search <query>  -> unified artists/albums/tracks
    main.py artist <name>   -> empty (archive.org has no artist pages here)
    main.py album  <id>     -> empty for the same reason
"""
import json
import sys
import urllib.parse
import urllib.request

UA = "mfe-player/0.2 (+https://github.com/omkargs/mfe-player)"
SEARCH = "https://archive.org/advancedsearch.php"
METADATA = "https://archive.org/metadata/{identifier}"
AUDIO_HINTS = ("mp3", "flac", "ogg", "opus", "m4a", "wav")
SKIP_HINTS = ("64kbps mono", "thumbnail")


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def seconds(length):
    """archive.org file lengths arrive as 'MM:SS' or 'HH:MM:SS'."""
    if not length or not isinstance(length, str):
        return None
    parts = length.split(":")
    if not all(part.isdigit() for part in parts):
        return None
    total = 0
    for part in parts:
        total = total * 60 + int(part)
    return total or None


def is_audio(info):
    fmt = str(info.get("format") or info.get("name") or "").lower()
    return any(hint in fmt for hint in AUDIO_HINTS) and not any(
        hint in fmt for hint in SKIP_HINTS
    )


def tracks_for(doc):
    identifier = doc.get("identifier")
    if not identifier:
        return []
    try:
        data = get(METADATA.format(identifier=urllib.parse.quote(identifier)))
    except Exception:
        return []
    collection_title = (doc.get("title") or "").strip()
    creator = doc.get("creator")
    if isinstance(creator, list):
        creator = ", ".join(str(part) for part in creator)
    creator = str(creator or "").strip()
    artwork = None
    for name in ("__ia_thumb.jpg", "itemimage.jpg", "thumbnail.jpg"):
        if any(str(info.get("name")) == name for info in data.get("files") or []):
            artwork = f"https://archive.org/download/{identifier}/{name}"
            break
    out, added = [], set()
    for info in data.get("files") or []:
        name = str(info.get("name") or "")
        if not is_audio(info) or name in added:
            continue
        added.add(name)
        title = str(info.get("title") or info.get("name") or "").strip()
        if not title:
            continue
        out.append(
            {
                "id": f"ia:{identifier}/{name}",
                "title": title,
                "artist": creator or (str(doc.get("collection") or "").strip()),
                "album": str(info.get("album") or collection_title).strip(),
                "duration": seconds(info.get("length")),
                "artwork": artwork,
                "source": "internet-archive",
                "year": str(doc.get("year") or "")[:4] or None,
            }
        )
        if len(out) >= 6:
            break
    return out


def cmd_search(query):
    params = {
        "q": f"(mediatype:(audio)) AND (title:({query}) OR creator:({query}) OR description:({query}))",
        "fl[]": ["identifier", "title", "creator", "year"],
        "rows": "8",
        "page": "1",
        "output": "json",
        "sort[]": "downloads desc",
    }
    url = f"{SEARCH}?{urllib.parse.urlencode(params, doseq=True)}"
    docs = (get(url).get("response") or {}).get("docs") or []
    tracks = []
    for doc in docs:
        tracks.extend(tracks_for(doc))
        if len(tracks) >= 24:
            break
    artists = sorted({track["artist"] for track in tracks if track["artist"]})
    return {"artists": [], "albums": [], "tracks": tracks, "_artists_seen": artists}


def main():
    if len(sys.argv) < 3:
        fail("usage: main.py search '<query>'")
    command = sys.argv[1]
    if command == "search":
        print(json.dumps({"artists": [], "albums": [], **cmd_search(sys.argv[2])}))
    else:
        print(json.dumps({"artists": [], "albums": [], "tracks": []}))


if __name__ == "__main__":
    main()