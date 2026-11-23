#!/usr/bin/env bash
# Builds the map regions a server hands out: for each region, the vector
# base map and the terrain of its box, as two PMTiles archives, and the
# map.toml that describes them, in a folder of its own. The folders go into
# the server's maps folder (maps_dir, VENTI_MAPS_DIR); docs/running-a-server.md,
# "Map regions", says where that is and how regions are renewed.
#
# The regions are the files in map/regions/: an id, a name and a box each
# (a region pack's region.toml has the same three and works too). They are
# all cut from two archives that are built once for the whole extract:
#
# - the base map: OpenStreetMap through Planetiler's stock OpenMapTiles
#   profile, from Geofabrik's extract of the Alps, limited to the extract's
#   outline, to zoom 14;
# - the terrain: Mapterhorn's elevation tiles over the outline's box, to
#   zoom 12, the level Mapterhorn has for the whole world.
#
# This is what tools/demo-map/build.sh does for the region the apps bundle.
# The project's own profile, contours and finer terrain are not built
# (docs/architecture.md, "Maps and terrain").
#
# A region's box should lie inside the extract's outline. Where it does
# not, the region has terrain (as far as the outline's box) and the coarse
# layers of the first zooms, and no streets or places.
#
# Needs Java 21 or later, curl, Python 3.11 or later, and go-pmtiles: the
# `pmtiles` on the PATH (Homebrew's formula `pmtiles`), or else Go, with
# which it is installed. Run from the repository root:
#
#   tools/region-map/build.sh <folder> [region.toml ...]
#
#   folder       where the regions' folders are made, <folder>/<id>. A
#                region whose folder is already there is refused: a region
#                is renewed by swapping folders, never in place.
#   region.toml  the regions to build. Unless given, all of map/regions/.
#
# Settings, as environment variables:
#
#   OSM_URL      the extract to download. Unless given, Geofabrik's Alps,
#                https://download.geofabrik.de/europe/alps/alps-latest.osm.pbf.
#                Its outline is the same address with .poly for -latest.osm.pbf.
#   OSM_PBF      an extract on this machine to use instead, and OSM_POLY its
#                outline (optional). Nothing is downloaded for them.
#   JAVA         the java to run. Unless given, the one on the PATH.
#   JAVA_MEMORY  the memory Planetiler's Java may take. Unless given, 8g.
#   VERSION      the version of the regions. Unless given, the day (UTC).
#
# The downloaded extract is kept in target/region-map and downloaded again
# when it is more than 25 days old, so a monthly run renews the map; the two
# archives of the whole extract are built again whenever the extract is newer
# than they are. Delete target/region-map/source to build them again anyway.
#
# What a run took on the owner's Mac, with each region's sizes, is in
# deploy/AGENTS.md, "Map regions".
set -euo pipefail

here="$(dirname "$0")"
maps="${1:?Usage: tools/region-map/build.sh <folder> [region.toml ...]}"
shift
if [ "$#" -eq 0 ]; then set -- map/regions/*.toml; fi
version="${VERSION:-$(date -u +%Y-%m-%d)}"
work="target/region-map"
source="$work/source"
mkdir -p "$work/bin" "$work/osm" "$source" "$maps"

for region in "$@"; do
  read -r id _ < <(python3 "$here/describe.py" box "$region")
  if [ -e "$maps/$id" ]; then
    echo "$maps/$id is already there. Build into another folder and swap the two." >&2
    exit 1
  fi
done

# Tools, as the demo script fetches them.
[ -f "$work/planetiler.jar" ] || curl -fL -o "$work/planetiler.jar" \
  https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar
pmtiles="$(command -v pmtiles || true)"
if [ -z "$pmtiles" ]; then
  [ -x "$work/bin/go-pmtiles" ] || GOBIN="$PWD/$work/bin" go install github.com/protomaps/go-pmtiles@latest
  pmtiles="$work/bin/go-pmtiles"
fi

# The extract and its outline.
if [ -n "${OSM_PBF:-}" ]; then
  osm="$OSM_PBF"
  poly="${OSM_POLY:-}"
else
  url="${OSM_URL:-https://download.geofabrik.de/europe/alps/alps-latest.osm.pbf}"
  osm="$work/osm/$(basename "$url")"
  poly="$work/osm/$(basename "${url%-latest.osm.pbf}").poly"
  if [ ! -f "$osm" ] || [ -n "$(find "$osm" -mtime +25)" ]; then
    curl -fL -o "$osm.part" "$url"
    mv "$osm.part" "$osm"
  fi
  [ -f "$poly" ] || curl -fL -o "$poly" "${url%-latest.osm.pbf}.poly"
fi

# The two archives of the whole extract, built when the extract is newer.
if [ ! -f "$source/basemap.pmtiles" ] || [ "$osm" -nt "$source/basemap.pmtiles" ]; then
  "${JAVA:-java}" "-Xmx${JAVA_MEMORY:-8g}" -jar "$work/planetiler.jar" \
    --osm-path="$osm" ${poly:+--polygon="$poly"} --download --tmpdir="$work/tmp" \
    --download_dir="$work/sources" --maxzoom=14 \
    --output="$source/basemap.pmtiles" --force
fi
if [ ! -f "$source/terrain.pmtiles" ] || [ "$osm" -nt "$source/terrain.pmtiles" ]; then
  if [ -n "$poly" ]; then
    outline="$(python3 "$here/describe.py" outline "$poly")"
  else
    outline="$("$pmtiles" show "$source/basemap.pmtiles" --header-json | python3 "$here/describe.py" outline -)"
  fi
  "$pmtiles" extract https://download.mapterhorn.com/planet.pmtiles \
    "$source/terrain.part.pmtiles" --bbox="$outline" --maxzoom=12 --download-threads=8
  mv "$source/terrain.part.pmtiles" "$source/terrain.pmtiles"
fi

# Each region, cut from the two. The three files go to their folder, the
# description last: a server that is started while they are on their way
# finds no region there, not half of one.
for region in "$@"; do
  read -r id bounds < <(python3 "$here/describe.py" box "$region")
  build="$work/build/$id"
  rm -rf "$build"
  mkdir -p "$build"
  echo "Cutting $id ($bounds), version $version"
  for file in basemap terrain; do
    "$pmtiles" extract "$source/$file.pmtiles" "$build/$file.pmtiles" --bbox="$bounds" --quiet
  done
  python3 "$here/describe.py" write "$region" "$build" "$version"
  mkdir -p "$maps/$id"
  mv "$build/basemap.pmtiles" "$build/terrain.pmtiles" "$maps/$id/"
  mv "$build/map.toml" "$maps/$id/map.toml"
  ls -l "$maps/$id"
done

echo "Done. A server lists the regions after its next start, once it has read"
echo "their files and found the hashes their map.toml states."
