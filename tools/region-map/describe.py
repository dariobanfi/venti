#!/usr/bin/env python3
"""What build.sh needs to know about a region pack, and the map.toml it ends with.

    describe.py box <region.toml>
        Prints the pack's identifier and its box as west,south,east,north:
        what the map tools are told to build.

    describe.py write <region.toml> <folder> <version>
        Writes <folder>/map.toml for the two archives in the folder,
        basemap.pmtiles and terrain.pmtiles, with the SHA-256 of each as it
        is on the disk. The server hands a region out only while those
        hashes are right (crates/server/src/maps.rs).

Needs Python 3.11 or later, for its reader of TOML.
"""

import hashlib
import json
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
        case ["write", region, folder, version]:
            write(region, folder, version)
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
