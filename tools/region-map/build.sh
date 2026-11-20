#!/usr/bin/env bash
# Builds one map region for a server to hand out: the vector base map and
# the terrain of a region pack's box, as two PMTiles archives, and the
# map.toml that describes them. The result is one folder for the server's
# maps folder (maps_dir, VENTI_MAPS_DIR). deploy/README.md says where that
# is and how a region is renewed.
#
# THIS SCRIPT HAS NOT BUILT A REGION YET. It was written on 2026-11-20 in a
# session that had neither the disk nor the memory for one. What was tried
# there: the script from its first line to its last with stand-ins for its
# three heavy steps (the download of OpenStreetMap, Planetiler, the terrain
# extract), and the folder it wrote, which `venti-server dev --maps` then
# listed. Every size below is an estimate or somebody else's figure, to
# verify with the first real run. How long a run takes is not known.
#
# It does what tools/demo-map/build.sh does, which built the region the
# apps bundle: the stock OpenMapTiles profile of Planetiler for the base
# map, and Mapterhorn's elevation tiles up to zoom 12 for the terrain. The
# project's own profile, contours and finer terrain are not built
# (docs/architecture.md, "Maps and terrain").
#
# Needs Java 21 or later, Go, curl and Python 3.11 or later. Run from the
# repository root:
#
#   tools/region-map/build.sh [region.toml] [folder]
#
#   region.toml  the region pack whose box is built. Unless given,
#                regions/alps-de-at/region.toml
#   folder       the region's folder, which is made. Unless given,
#                var/maps/<the pack's id>
#
# Settings, as environment variables:
#
#   OSM_PBF      an OpenStreetMap extract (.osm.pbf) on this machine that
#                covers the whole box. Unless given, the extract of Europe
#                is downloaded, once.
#   JAVA_MEMORY  the memory Planetiler's Java may take. Unless given, 18g.
#   VERSION      the version of the region. Unless given, the day (UTC).
#
# What a run takes, all to verify:
#
# - OpenStreetMap. The extract of Europe on the mirror that this script
#   and the demo script download from was 35.5 GB on 2026-11-19, by the
#   mirror's own listing, read on 2026-11-20. Planetiler's README (read the
#   same day) says "Give the JVM about 0.5x the size of the input .osm.pbf
#   file" and asks for "at least 1GB of free SSD disk space plus 5-10x the
#   size of the .osm.pbf file". For Europe that makes 18 GB of memory for
#   Java, and between 180 and 360 GB of disk beside the 35.5 GB of the
#   file itself. Whether Planetiler needs as much when it is told to build
#   a box only, as it is here, is not known.
#   A smaller extract that covers the box makes all of that smaller in
#   proportion. Geofabrik publishes extracts of single countries; whether
#   it has one that covers the first region's box, which reaches from Lake
#   Constance to east of Vienna and into eight countries, was not checked:
#   download.geofabrik.de could not be reached from the session that wrote
#   this. An extract that covers the box in part gives a map that ends
#   where the extract ends, and no error.
# - Planetiler's other sources, which it downloads itself: about 1 GB. The
#   demo script's notes speak of roughly 6 GB of downloads, of which the
#   extract of Germany was 4.9 GB by the same listing.
# - The two archives. Scaled up by area from the region the apps bundle
#   (3.2 MB and 4.2 MB for 0.035 square degrees), the first region's box
#   of 17 square degrees would come to about 1.6 GB of base map and 2.1 GB
#   of terrain. That is arithmetic, not a measurement: towns weigh more
#   than the valley the demo covers and high mountains less.
# - Mapterhorn. The terrain is cut out of its archive of the whole planet
#   over the network: this downloads about as much as the terrain archive
#   holds. Mapterhorn's terms for use at this volume, and for a server
#   that hands an extract on to many pilots, are to verify with that
#   project before a public release: docs/architecture.md says the same of
#   its tile service.
set -euo pipefail

region="${1:-regions/alps-de-at/region.toml}"
here="$(dirname "$0")"
read -r id bounds < <(python3 "$here/describe.py" box "$region")
out="${2:-var/maps/$id}"
version="${VERSION:-$(date -u +%Y-%m-%d)}"
work="target/region-map"
build="$work/$id"
mkdir -p "$work/bin" "$build"
echo "Building the map of $id ($bounds), version $version, into $out"

# Tools, as the demo script fetches them.
[ -f "$work/planetiler.jar" ] || curl -fL -o "$work/planetiler.jar" \
  https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar
[ -x "$work/bin/go-pmtiles" ] || GOBIN="$PWD/$work/bin" go install github.com/protomaps/go-pmtiles@latest

# Terrain: an extract of the Mapterhorn elevation tiles, to the zoom it
# has for the whole world.
"$work/bin/go-pmtiles" extract https://download.mapterhorn.com/planet.pmtiles \
  "$build/terrain.pmtiles" --bbox="$bounds" --maxzoom=12

# Vector base map: OpenStreetMap through Planetiler's OpenMapTiles profile.
# The extract of Europe comes from a mirror of Geofabrik's extracts.
osm="${OSM_PBF:-$work/europe.osm.pbf}"
[ -f "$osm" ] || curl -fL -o "$osm" \
  https://ftp5.gwdg.de/pub/misc/openstreetmap/download.geofabrik.de/europe-latest.osm.pbf
java "-Xmx${JAVA_MEMORY:-18g}" -jar "$work/planetiler.jar" \
  --osm-path="$osm" --download --tmpdir="$work/tmp" \
  --download_dir="$work/sources" \
  --bounds="$bounds" --maxzoom=14 \
  --output="$build/basemap.pmtiles" --force

# The description, with the hash of each archive as it now is. Then the
# three files go to their folder, the description last: a server that is
# started while they are on their way finds no region there, not half of
# one.
python3 "$here/describe.py" write "$region" "$build" "$version"
mkdir -p "$out"
mv "$build/basemap.pmtiles" "$build/terrain.pmtiles" "$out/"
mv "$build/map.toml" "$out/map.toml"

ls -l "$out"
echo "Done. A server lists the region after its next start, once it has read"
echo "the files and found these hashes. If $out is the folder of a region the"
echo "server is handing out now, it stops handing that one out until then."
