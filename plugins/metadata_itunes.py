#!/usr/bin/env python3
"""MFE Player metadata plugin: Apple iTunes Search API.

Protocol:
    main.py search <query>     -> unified artists/albums/tracks
    main.py artist <id|name>   -> an artist page (albums + top tracks)
    main.py album <id>         -> one album with its tracks

All commands print a JSON object:
    {"artists": [...], "albums": [...], "tracks": [...]}
"""
import json
import sys
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

UA = "mfe-player/0.2 (+https://github.com/omkargs/mfe-player)"
API = "https://itunes.apple.com"


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get_json(url):
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=12) as response:
        return json.load(response)


def search(term, entity, limit, attribute=None):
    params = {"term": term, "entity": entity, "media": "music", "limit": limit}
    if attribute:
        params["attribute"] = attribute
    url = f"{API}/search?{urllib.parse.urlencode(params)}"
    try:
        return get_json(url).get("results", [])
    except Exception:
        return []


def lookup(item_id, entity, limit):
    params = {"id": item_id, "entity": entity, "media": "music", "limit": limit}
    url = f"{API}/lookup?{urllib.parse.urlencode(params)}"
    try:
        return get_json(url).get("results", [])
    except Exception:
        return []


def artwork_url(item, key="artworkUrl100"):
    artwork = item.get(key) or item.get("artworkUrl60") or ""
    if not artwork:
        return None
    return (
        artwork.replace("100x100bb", "1000x1000bb")
        .replace("60x60bb", "1000x1000bb")
        .replace("30x30bb", "1000x1000bb")
    )


def track_from(item):
    title = (item.get("trackName") or "").strip()
    artist = (item.get("artistName") or "").strip()
    if not title or not artist:
        return None
    millis = item.get("trackTimeMillis")
    duration = int(millis / 1000) if millis else None
    release = item.get("releaseDate") or ""
    return {
        "id": f"itunes:{item.get('trackId')}",
        "title": title,
        "artist": artist,
        "album": (item.get("collectionName") or "").strip(),
        "duration": duration,
        "artwork": artwork_url(item),
        "source": "itunes",
        "year": release[:4] if release else None,
    }


def album_from(item):
    title = (item.get("collectionName") or "").strip()
    artist = (item.get("artistName") or "").strip()
    if not title or not artist:
        return None
    release = item.get("releaseDate") or ""
    return {
        "id": f"itunes-album:{item.get('collectionId')}",
        "title": title,
        "artist": artist,
        "artwork": artwork_url(item),
        "year": release[:4] if release else None,
        "track_count": item.get("trackCount"),
        "source": "itunes",
        "tracks": [],
    }


def artist_from(item):
    name = (item.get("artistName") or "").strip()
    if not name:
        return None
    return {
        "id": f"itunes-artist:{item.get('artistId')}",
        "name": name,
        "artwork": None,
        "source": "itunes",
        "album_count": None,
    }


def gather_tracks(results):
    out, seen = [], set()
    for item in results:
        if item.get("kind") not in (None, "song"):
            continue
        track = track_from(item)
        if track and track["id"] not in seen:
            seen.add(track["id"])
            out.append(track)
    return out


def gather_albums(results):
    out, seen = [], set()
    for item in results:
        if item.get("collectionType") != "Album" and item.get("wrapperType") != "collection":
            continue
        if not item.get("collectionName"):
            continue
        album = album_from(item)
        if album and album["id"] not in seen:
            seen.add(album["id"])
            out.append(album)
    return out


def gather_artists(results):
    out, seen = [], set()
    for item in results:
        if item.get("wrapperType") != "artist" and item.get("artistType") is None:
            continue
        artist = artist_from(item)
        if artist and artist["id"] not in seen:
            seen.add(artist["id"])
            out.append(artist)
    return out


def cmd_search(query):
    with ThreadPoolExecutor(max_workers=3) as pool:
        songs_f = pool.submit(search, query, "song", 25)
        albums_f = pool.submit(search, query, "album", 10)
        artists_f = pool.submit(search, query, "musicArtist", 6)
        songs = gather_tracks(songs_f.result())
        albums = gather_albums(albums_f.result())
        artists = gather_artists(artists_f.result())

    lowered = query.lower()
    artists.sort(
        key=lambda a: (
            0 if a["name"].lower() == lowered else 1,
            0 if lowered in a["name"].lower() else 1,
            a["name"].lower(),
        )
    )
    return {"artists": artists, "albums": albums, "tracks": songs}


def numeric_id(raw):
    return raw.rsplit(":", 1)[-1].strip()


def cmd_artist(arg):
    arg = arg.strip()
    artists = []
    if arg.startswith("itunes-artist:"):
        artist_id = numeric_id(arg)
        with ThreadPoolExecutor(max_workers=2) as pool:
            songs_f = pool.submit(lookup, artist_id, "song", 60)
            albums_f = pool.submit(lookup, artist_id, "album", 30)
            song_results = songs_f.result()
            tracks = gather_tracks(song_results)
            albums = gather_albums(albums_f.result())
            artists = gather_artists(song_results)
    else:
        # Resolve the best matching artist name, then browse their catalog.
        artists = gather_artists(search(arg, "musicArtist", 6))
        lowered = arg.lower()
        artists.sort(
            key=lambda a: (
                0 if a["name"].lower() == lowered else 1,
                0 if lowered in a["name"].lower() else 1,
            )
        )
        if artists:
            artist_id = numeric_id(artists[0]["id"])
            with ThreadPoolExecutor(max_workers=2) as pool:
                songs_f = pool.submit(lookup, artist_id, "song", 60)
                albums_f = pool.submit(lookup, artist_id, "album", 30)
                tracks = gather_tracks(songs_f.result())
                albums = gather_albums(albums_f.result())
        else:
            tracks = gather_tracks(search(arg, "song", 50, "artistTerm"))
            albums = gather_albums(search(arg, "album", 20))
        artists = artists[:1]

    return {"artists": artists, "albums": albums, "tracks": tracks}


def cmd_album(arg):
    album_id = numeric_id(arg)
    results = lookup(album_id, "song", 200)
    tracks = gather_tracks(results)
    albums = gather_albums(results)
    if not albums and tracks:
        first = tracks[0]
        albums = [
            {
                "id": f"itunes-album:{album_id}",
                "title": first.get("album") or "",
                "artist": first.get("artist") or "",
                "artwork": first.get("artwork"),
                "year": first.get("year"),
                "track_count": len(tracks),
                "source": "itunes",
                "tracks": [],
            }
        ]
    if albums:
        albums[0]["tracks"] = tracks
    return {"artists": [], "albums": albums, "tracks": tracks}


def main():
    if len(sys.argv) < 3:
        fail("usage: main.py <search|artist|album> <argument>")
    command = sys.argv[1]
    argument = " ".join(sys.argv[2:]).strip()
    if not argument:
        print(json.dumps({"artists": [], "albums": [], "tracks": []}))
        return

    try:
        if command == "search":
            payload = cmd_search(argument)
        elif command == "artist":
            payload = cmd_artist(argument)
        elif command == "album":
            payload = cmd_album(argument)
        else:
            fail(f"unknown command: {command}")
    except Exception as exc:
        fail(f"iTunes request failed: {exc}")

    print(json.dumps(payload, ensure_ascii=False))


if __name__ == "__main__":
    main()
