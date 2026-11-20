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

# Label fonts: Noto Sans, as glyph ranges. The map engine asks for the range
# of every character a label holds, and on the phones a range that is not
# there is never answered: the tile the label belongs to then stays unfinished
# and nothing of it is drawn. So every range of the Basic Multilingual Plane
# has a file. Latin, Greek, Cyrillic, punctuation and the common symbols are
# the real glyphs; every other range is a file that says "no glyphs here", as
# the source itself publishes for ranges the font does not cover. A label in
# another script is then left out, and nothing waits for it.
fonts="https://protomaps.github.io/basemaps-assets/fonts"
real="0-255 256-511 512-767 768-1023 1024-1279 1280-1535 7680-7935 7936-8191 8192-8447 8448-8703 8704-8959 9472-9727 9728-9983"
for range in $real; do
  curl -fL -o "$out/fonts/noto-sans-regular/$range.pbf" "$fonts/Noto%20Sans%20Regular/$range.pbf"
done
python3 "$(dirname "$0")/empty-glyph-ranges.py" "$out/fonts/noto-sans-regular" $real
curl -fL -o "$out/fonts/OFL.txt" \
  https://raw.githubusercontent.com/protomaps/basemaps-assets/main/fonts/OFL.txt

# The description a server hands this region out with, map.toml, states the
# edition of the two archives and the SHA-256 of each, and a server lists
# the region only while those are right. The archives are new, so both are
# written again: the day as the version, and each hash on the line after
# its file's name.
sha256() {
  if command -v sha256sum > /dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}
description="$out/map.toml"
sed -i.bak "s/^version = \".*\"/version = \"$(date -u +%Y-%m-%d)\"/" "$description"
for file in basemap.pmtiles terrain.pmtiles; do
  sed -i.bak "/^name = \"$file\"/{n;s/^sha256 = \".*\"/sha256 = \"$(sha256 "$out/$file")\"/;}" "$description"
done
rm -f "$description.bak"

ls -l "$out"
echo "The server's tests keep the list of this region, which is also the example"
echo "in docs/api.md. Accept the new list, then copy the file into the document:"
echo "  VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server --test maps"
echo "  crates/server/tests/golden/maps.json"
