#!/usr/bin/env bash
# Rebuilds the small map region bundled with the apps for development and
# tests: the Tegernsee valley, where the sample flight takes place.
#
# This is not the production map pipeline. It uses the stock OpenMapTiles
# profile and terrain up to zoom 12 only, to keep the bundled files small.
#
# Needs Java 21 or later, Go and curl, about 8 GB of free disk and roughly
# 6 GB of downloads. Run from the repository root:
#
#   tools/demo-map/build.sh
set -euo pipefail

bounds="11.64,47.60,11.86,47.76"
work="target/demo-map"
out="apps/ios/Venti/Resources/MapDemo"
mkdir -p "$work/bin" "$out/fonts/noto-sans-regular"

# Tools.
[ -f "$work/planetiler.jar" ] || curl -fL -o "$work/planetiler.jar" \
  https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar
[ -x "$work/bin/go-pmtiles" ] || GOBIN="$PWD/$work/bin" go install github.com/protomaps/go-pmtiles@latest

# Terrain: an extract of the Mapterhorn elevation tiles.
"$work/bin/go-pmtiles" extract https://download.mapterhorn.com/planet.pmtiles \
  "$out/terrain.pmtiles" --bbox="$bounds" --maxzoom=12

# Vector base map: OpenStreetMap through Planetiler's OpenMapTiles profile.
# The Germany file comes from a mirror of Geofabrik's extracts.
[ -f "$work/germany.osm.pbf" ] || curl -fL -o "$work/germany.osm.pbf" \
  https://ftp5.gwdg.de/pub/misc/openstreetmap/download.geofabrik.de/germany-latest.osm.pbf
java -Xmx8g -jar "$work/planetiler.jar" \
  --osm-path="$work/germany.osm.pbf" --download --tmpdir="$work/tmp" \
  --download_dir="$work/sources" \
  --bounds="$bounds" --maxzoom=14 \
  --output="$out/basemap.pmtiles" --force

# Label fonts: Noto Sans, as glyph ranges covering Latin text.
fonts="https://protomaps.github.io/basemaps-assets/fonts"
for range in 0-255 256-511; do
  curl -fL -o "$out/fonts/noto-sans-regular/$range.pbf" "$fonts/Noto%20Sans%20Regular/$range.pbf"
done
curl -fL -o "$out/fonts/OFL.txt" \
  https://raw.githubusercontent.com/protomaps/basemaps-assets/main/fonts/OFL.txt

ls -l "$out"
