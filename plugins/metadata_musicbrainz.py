#!/usr/bin/env python3
"""Nuclear TUI metadata plugin: MusicBrainz.

Protocol:
    main.py search <query>     -> unified artists/albums/tracks
    main.py artist <id|name>   -> an artist page
    main.py album <id>         -> one release with its tracks
"""
import json
import sys
import urllib.parse
import urllib.request

UA = "MfePlayer/0.2 (https://github.com/mfe-player)"
API = "https://musicbrainz.org/ws/2"


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get_json(path, params):
    url = f"{API}/{path}?{urllib.parse.urlencode(params)}"
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=15) as response:
        return json.load(response)


def artist_name(recording):
    credits = recording.get("artist-credit") or []
    return " & ".join(
        (credit.get("name") or credit.get("artist", {}).get("name") or "").strip()
        for credit in credits
    ).strip()


def artist_id(recording):
    credits = recording.get("artist-credit") or []
    for credit in credits:
        value = (credit.get("artist") or {}).get("id")
        if value:
            return value
    return None


def track_from(recording, album=None, artwork=None):
    title = (recording.get("title") or "").strip()
    artist = artist_name(recording)
    if not title or not artist:
        return None
    length = recording.get("length")
    releases = recording.get("releases") or []
    if album is None:
        album = (releases[0].get("title") or "").strip() if releases else ""
    if artwork is None and releases and releases[0].get("id"):
        artwork = f"https://coverartarchive.org/release/{releases[0]['id']}/front-500"
    return {
        "id": f"musicbrainz:{recording.get('id')}",
        "title": title,
        "artist": artist,
        "album": album,
        "duration": int(length / 1000) if length else None,
        "artwork": artwork,
        "source": "musicbrainz",
        "year": (recording.get("first-release-date") or "")[:4] or None,
    }


def synth(recordings):
    tracks, artists, albums = [], [], []
    seen_tracks, seen_artists, seen_albums = set(), set(), set()
    for recording in recordings:
        track = track_from(recording)
        if track and track["id"] not in seen_tracks:
            seen_tracks.add(track["id"])
            tracks.append(track)

        name = artist_name(recording)
        aid = artist_id(recording)
        key = aid or name
        if name and key not in seen_artists:
            seen_artists.add(key)
            artists.append(
                {
                    "id": f"musicbrainz-artist:{aid}" if aid else f"musicbrainz-artist:{name}",
                    "name": name,
                    "artwork": None,
                    "source": "musicbrainz",
                    "album_count": None,
                }
            )

        for release in (recording.get("releases") or [])[:1]:
            title = (release.get("title") or "").strip()
            rid = release.get("id")
            if not title or not rid or rid in seen_albums:
                continue
            seen_albums.add(rid)
            albums.append(
                {
                    "id": f"musicbrainz-release:{rid}",
                    "title": title,
                    "artist": name,
                    "artwork": f"https://coverartarchive.org/release/{rid}/front-500",
                    "year": (release.get("date") or "")[:4] or None,
                    "track_count": release.get("track-count"),
                    "source": "musicbrainz",
                    "tracks": [],
                }
            )
    return {"artists": artists, "albums": albums, "tracks": tracks}


def cmd_search(query):
    data = get_json(
        "recording", {"query": query, "fmt": "json", "limit": 30}
    )
    return synth(data.get("recordings", []))


def command_artist(argument):
    argument = argument.strip()
    if argument.startswith("musicbrainz-artist:"):
        value = argument.split(":", 1)[1]
        if len(value) == 36 and value.count("-") == 4:
            query = f"arid:{value}"
        else:
            query = f'artist:"{value}"'
    else:
        query = f'artist:"{argument}"'
    data = get_json("recording", {"query": query, "fmt": "json", "limit": 60})
    return synth(data.get("recordings", []))


def command_album(argument):
    mbid = argument.rsplit(":", 1)[-1].strip()
    data = get_json(f"release/{mbid}", {"inc": "recordings+artist-credits", "fmt": "json"})
    title = (data.get("title") or "").strip()
    artist = " & ".join(
        (credit.get("name") or credit.get("artist", {}).get("name") or "").strip()
        for credit in (data.get("artist-credit") or [])
    ).strip()
    tracks, seen = [], set()
    for medium in data.get("media") or []:
        for item in medium.get("tracks") or []:
            recording = item.get("recording") or {}
            track = track_from(
                recording,
                album=title,
                artwork=f"https://coverartarchive.org/release/{mbid}/front-500",
            )
            if track and track["id"] not in seen:
                seen.add(track["id"])
                tracks.append(track)
    album = {
        "id": f"musicbrainz-release:{mbid}",
        "title": title,
        "artist": artist,
        "artwork": f"https://coverartarchive.org/release/{mbid}/front-500",
        "year": (data.get("date") or "")[:4] or None,
        "track_count": len(tracks),
        "source": "musicbrainz",
        "tracks": [],
    }
    if tracks:
        album["tracks"] = tracks
    return {"artists": [], "albums": [album] if title else [], "tracks": tracks}


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
            payload = command_artist(argument)
        elif command == "album":
            payload = command_album(argument)
        else:
            fail(f"unknown command: {command}")
    except Exception as exc:
        fail(f"MusicBrainz request failed: {exc}")
    print(json.dumps(payload, ensure_ascii=False))


if __name__ == "__main__":
    main()
