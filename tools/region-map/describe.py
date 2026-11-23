#!/usr/bin/env python3
"""What build.sh needs to know about a region, and the map.toml it ends with.

    describe.py box <region.toml>
        Prints the region's identifier and its box as west,south,east,north:
        what the map tools are told to build. A region is a file of
        map/regions/ or a region pack's region.toml: both have an id, a name
        and a box.

    describe.py outline <extract.poly | ->
        Prints the box around an extract, west,south,east,north, widened to
        the next hundredth of a degree: from the outline Geofabrik publishes
        beside it (Osmosis's .poly), or from `pmtiles show --header-json` of
        the base map on standard input.

    describe.py write <region.toml> <folder> <version>
        Writes <folder>/map.toml for the two archives in the folder,
        basemap.pmtiles and terrain.pmtiles, with the SHA-256 of each as it
        is on the disk. The server hands a region out only while those
        hashes are right (crates/server/src/maps.rs).

Needs Python 3.11 or later, for its reader of TOML.
"""

import hashlib
import json
import math
import pathlib
import sys
import tomllib

# The files build.sh makes, each with what it is and whose data it holds.
# The credits are those of the region bundled with the apps
# (apps/ios/Venti/Resources/MapDemo/ATTRIBUTION.txt), which is built from
# the same two sources by the same tools. They stay apart per file: the
# two archives hold data under different licences and are never merged.
FILES = [
    (
        "basemap",
        "basemap.pmtiles",
        "OpenStreetMap's data under the Open Database License 1.0, in the tile\n"
        "# schema of OpenMapTiles, which is under CC BY 4.0.",
        [
            ("© OpenStreetMap contributors", "https://www.openstreetmap.org/copyright"),
            ("© OpenMapTiles", "https://openmaptiles.org"),
        ],
    ),
    (
        "terrain",
        "terrain.pmtiles",
        "Mapterhorn's elevation tiles. Its attribution page lists the surveys.",
        [
            (
                "© Mapterhorn and the surveys it combines",
                "https://mapterhorn.com/attribution",
            ),
        ],
    ),
]


def pack(path):
    with open(path, "rb") as file:
        return tomllib.load(file)


def text(value):
    """A string as TOML writes it. JSON's spelling of a string is one of TOML's."""
    return json.dumps(value, ensure_ascii=False)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        while chunk := file.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def box(region):
    bounds = pack(region)["bounds"]
    edges = ",".join(str(bounds[edge]) for edge in ("west", "south", "east", "north"))
    print(pack(region)["id"], edges)


def outline(path):
    if path == "-":
        west, south, east, north = json.load(sys.stdin)["bounds"]
    else:
        # A name, then rings: a name line, a "longitude latitude" line per
        # point, and END; the file ends with one more END.
        points = []
        for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines()[1:]:
            words = line.split()
            if len(words) == 2:
                points.append((float(words[0]), float(words[1])))
        west = min(lon for lon, _ in points)
        east = max(lon for lon, _ in points)
        south = min(lat for _, lat in points)
        north = max(lat for _, lat in points)
    down = lambda value: math.floor(value * 100) / 100
    up = lambda value: math.ceil(value * 100) / 100
    print(f"{down(west)},{down(south)},{up(east)},{up(north)}")


def write(region, folder, version):
    region = pack(region)
    folder = pathlib.Path(folder)
    bounds = region["bounds"]
    covers = ", ".join(f"{edge} = {bounds[edge]}" for edge in ("south", "west", "north", "east"))
    lines = [
        "# Written by tools/region-map/build.sh. The server reads this file when it",
        "# starts and hands the region out only while each file has the hash stated",
        "# here: after changing a file, build the region again or correct its hash.",
        "",
        f"id = {text(region['id'])}",
        f"name = {text(region['name'])}",
        f"version = {text(version)}",
        f"covers = {{ {covers} }}",
    ]
    for role, name, about, credits in FILES:
        lines += [
            "",
            f"# {about}",
            "[[files]]",
            f"role = {text(role)}",
            f"name = {text(name)}",
            f"sha256 = {text(sha256(folder / name))}",
            "credits = [",
        ]
        lines += [f"  {{ text = {text(credit)}, url = {text(url)} }}," for credit, url in credits]
        lines += ["]"]
    (folder / "map.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main(arguments):
    match arguments:
        case ["box", region]:
            box(region)
        case ["outline", path]:
            outline(path)
        case ["write", region, folder, version]:
            write(region, folder, version)
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
