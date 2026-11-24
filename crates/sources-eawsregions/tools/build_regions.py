# Makes assets/regions.geojson, the Alpine avalanche warning micro-regions
# of the EAWS (CC0 1.0), from the files of gitlab.com/eaws/eaws-regions:
# see the manifest's [[asset]] for the commit and where they are
# downloaded.
#
# Not run by the build or the tests. Needs Python 3 alone, nothing to
# install. Takes the folder the files were downloaded into, the commit
# they are of, the day that decides which regions are current, and the
# file to write:
#
#   python3 -I tools/build_regions.py <folder> <commit> <yyyy-mm-dd> assets/regions.geojson
#
# The folder holds micro-regions/<provider>.geojson.json, one file per
# provider (public/micro-regions/<provider>_micro-regions.geojson.json of
# the repository), and names/<language>.json (public/micro-regions_names).
#
# A region is current when it has no start_date after the day and no
# end_date on or before it. Each ring is simplified by Douglas-Peucker
# with a tolerance of 0.001 degrees (about 100 m) and rounded to four
# decimals (some 10 m); a ring left with less than about a hectare is
# dropped. Rings are simplified each on its own, so two neighbours' common
# border may part or cross by up to the tolerance. Each region gets a
# label point: the point inside its largest polygon furthest from its
# edges, found on a grid and refined.
import json
import os
import sys

PROVIDERS = [
    "AT-02", "AT-03", "AT-04", "AT-05", "AT-06", "AT-07", "AT-08",
    "CH", "DE-BY", "FR",
    "IT-21", "IT-23", "IT-25", "IT-32-BZ", "IT-32-TN", "IT-34", "IT-36",
    "SI",
]
LANGUAGES = ["de", "it", "en", "fr", "sl"]
TOLERANCE = 0.001
DECIMALS = 4
SMALLEST_RING = 0.000001  # square degrees, about a hectare at 46° N
# France's file has the Pyrenees and Corsica too: of it only the regions
# whose label lies east of this longitude and north of this latitude, the
# Alps.
FRANCE_ALPS_EAST_OF = 5.0
FRANCE_ALPS_NORTH_OF = 43.5


def simplify(points):
    """Douglas-Peucker on a closed ring (first point not repeated)."""
    n = len(points)
    if n < 4:
        return points
    far = max(range(n), key=lambda i: (points[i][0] - points[0][0]) ** 2 + (points[i][1] - points[0][1]) ** 2)
    keep = [False] * n
    keep[0] = keep[far] = True
    stack = [(0, far), (far, n)]
    while stack:
        a, b = stack.pop()
        if b - a < 2:
            continue
        ax, ay = points[a]
        bx, by = points[b % n]
        dx, dy = bx - ax, by - ay
        length = (dx * dx + dy * dy) ** 0.5
        worst, at = -1.0, None
        for i in range(a + 1, b):
            px, py = points[i]
            if length == 0:
                d = ((px - ax) ** 2 + (py - ay) ** 2) ** 0.5
            else:
                d = abs(dy * (px - ax) - dx * (py - ay)) / length
            if d > worst:
                worst, at = d, i
        if worst > TOLERANCE:
            keep[at] = True
            stack.append((a, at))
            stack.append((at, b))
    return [p for p, k in zip(points, keep) if k]


def area(points):
    return 0.5 * sum(points[i - 1][0] * p[1] - p[0] * points[i - 1][1] for i, p in enumerate(points))


def inside(x, y, rings):
    crossings = False
    for ring in rings:
        for i, (bx, by) in enumerate(ring):
            ax, ay = ring[i - 1]
            if (ay > y) != (by > y) and x < ax + (y - ay) * (bx - ax) / (by - ay):
                crossings = not crossings
    return crossings


def distance_to_edges(x, y, rings):
    best = float("inf")
    for ring in rings:
        for i, (bx, by) in enumerate(ring):
            ax, ay = ring[i - 1]
            dx, dy = bx - ax, by - ay
            length = dx * dx + dy * dy
            t = 0.0 if length == 0 else max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / length))
            px, py = ax + t * dx, ay + t * dy
            best = min(best, ((x - px) ** 2 + (y - py) ** 2) ** 0.5)
    return best


def label(polygon):
    """A point inside the polygon (outer ring and holes) far from its edges."""
    xs = [p[0] for p in polygon[0]]
    ys = [p[1] for p in polygon[0]]
    west, east, south, north = min(xs), max(xs), min(ys), max(ys)
    best = None
    steps = 24
    for _ in range(4):
        dx, dy = (east - west) / steps, (north - south) / steps
        for i in range(steps + 1):
            for j in range(steps + 1):
                x, y = west + i * dx, south + j * dy
                if not inside(x, y, polygon):
                    continue
                d = distance_to_edges(x, y, polygon)
                if best is None or d > best[0]:
                    best = (d, x, y)
        if best is None:
            break
        _, x, y = best
        west, east = x - 2 * dx, x + 2 * dx
        south, north = y - 2 * dy, y + 2 * dy
    if best is None:
        x, y = polygon[0][0]
        return [round(x, DECIMALS), round(y, DECIMALS)]
    return [round(best[1], DECIMALS), round(best[2], DECIMALS)]


def current(properties, day):
    start, end = properties.get("start_date"), properties.get("end_date")
    return (not start or start <= day) and (not end or end > day)


def main():
    folder, commit, day, target = sys.argv[1:5]
    names = {}
    for language in LANGUAGES:
        with open(os.path.join(folder, "names", language + ".json"), encoding="utf-8") as f:
            names[language] = json.load(f)
    features = []
    for provider in PROVIDERS:
        with open(os.path.join(folder, "micro-regions", provider + ".geojson.json"), encoding="utf-8") as f:
            collection = json.load(f)
        for feature in collection["features"]:
            properties = feature["properties"]
            if not current(properties, day):
                continue
            region = properties["id"]
            geometry = feature["geometry"]
            polygons = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
            kept = []
            for polygon in polygons:
                rings = []
                for number, ring in enumerate(polygon):
                    points = [(float(p[0]), float(p[1])) for p in ring]
                    if points[0] == points[-1]:
                        points = points[:-1]
                    simplified = simplify(points)
                    if len(simplified) < 3:
                        simplified = points
                    rounded = [(round(x, DECIMALS), round(y, DECIMALS)) for x, y in simplified]
                    rounded = [p for i, p in enumerate(rounded) if p != rounded[i - 1]]
                    if len(rounded) < 3 or abs(area(rounded)) < SMALLEST_RING:
                        if number == 0:
                            break
                        continue
                    rings.append(rounded)
                if rings:
                    kept.append(rings)
            if not kept:
                print("left out, nothing left of it:", region)
                continue
            largest = max(kept, key=lambda rings: abs(area(rings[0])))
            at = label(largest)
            if provider == "FR" and (at[0] < FRANCE_ALPS_EAST_OF or at[1] < FRANCE_ALPS_NORTH_OF):
                continue
            region_names = {
                language: names[language][region]
                for language in LANGUAGES
                if names[language].get(region)
            }
            features.append({
                "type": "Feature",
                "properties": {"id": region, "names": region_names, "label": at},
                "geometry": {
                    "type": "MultiPolygon",
                    "coordinates": [[[list(p) for p in ring] + [list(ring[0])] for ring in rings] for rings in kept],
                },
            })
    features.sort(key=lambda f: f["properties"]["id"])
    ids = [f["properties"]["id"] for f in features]
    if len(set(ids)) != len(ids):
        raise SystemExit("a region twice: " + " ".join(sorted({i for i in ids if ids.count(i) > 1})))
    unnamed = [i for i, f in zip(ids, features) if "en" not in f["properties"]["names"] and "de" not in f["properties"]["names"]]
    if unnamed:
        print("without an English or German name:", " ".join(unnamed))
    version = "eaws-regions " + commit[:8] + " " + day
    collection = {
        "type": "FeatureCollection",
        "version": version,
        "features": features,
    }
    with open(target, "w", encoding="utf-8") as out:
        json.dump(collection, out, separators=(",", ":"), ensure_ascii=False)
    print(len(features), "regions; version", version, "; size", os.path.getsize(target), "bytes")


main()
