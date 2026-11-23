# Makes assets/zones.bin, the time zone of every place on land, from
# timezone-boundary-builder's "same since 1970" outlines without oceans
# (ODbL, from OpenStreetMap): see the manifest's [[asset]] for the release
# and where it is downloaded.
#
# Not run by the build or the tests. Needs Python 3 alone, nothing to
# install. Takes the GeoJSON unpacked from timezones-1970.geojson.zip and
# the file to write:
#
#   python3 -I tools/build_zones.py <combined-1970.json> assets/zones.bin
#
# `--check` then also compares the asset with the full outlines at random
# places on land and prints how far from a border a wrong answer was.
# `--tolerance=0.0005` simplifies by another tolerance than TOLERANCE: on
# release 2026f, 0.002° made 0.85 MB, 0.001° 1.12 MB and 0.0005° 1.48 MB.
#
# The world is cut into cells of CELL degrees. A cell that one zone covers
# whole, or one zone and the sea, is that zone; a cell with no zone is
# none. Only a cell that two zones or more share keeps outlines: those of
# every zone but the one with the most of the cell, which is what is left.
# So the sea is the coast's zone up to a cell's width from land, and the
# coastlines cost nothing. Each ring is simplified by Douglas-Peucker with a
# tolerance of TOLERANCE degrees before it is cut, and its points are kept
# to 1/QUANTUM of a degree: a place is given the wrong zone only within
# about TOLERANCE of a border between two zones.
#
# The file, all numbers LEB128 varints, signed ones zigzagged:
#   "VTZ1", the cell's size in 1/QUANTUM degrees, the number of zones and
#   each zone's name (its length in bytes, then UTF-8);
#   for each row of cells, south to north from 90° S: the number of runs,
#   then each run's length in cells (west to east from 180° W) and its
#   code: 0 no zone, 1 to N the zone of that number less one, N + 1 a cell
#   with outlines, numbered in the order they come;
#   for each cell with outlines: its length in bytes, the zone that is left
#   (code as above), the number of zones with outlines, and for each its
#   number, its rings, and for each ring its points, each as the
#   difference to the one before from the cell's south-west corner, in
#   1/QUANTUM degrees, longitude first. A place is in a zone when a line
#   from it crosses that zone's rings an odd number of times.
import json
import math
import os
import random
import sys

CELL = 0.5
TOLERANCE = 0.001
QUANTUM = 10000
MAGIC = b"VTZ1"


def simplify(points, tolerance):
    """Douglas-Peucker on a closed ring (last point not repeated), without
    recursion."""
    n = len(points)
    if n < 4:
        return points
    x0, y0 = points[0]
    far = max(range(n), key=lambda i: (points[i][0] - x0) ** 2 + (points[i][1] - y0) ** 2)
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
        length = math.hypot(dx, dy)
        worst, at = -1.0, None
        if length == 0:
            for i in range(a + 1, b):
                px, py = points[i]
                d = math.hypot(px - ax, py - ay)
                if d > worst:
                    worst, at = d, i
        else:
            for i in range(a + 1, b):
                px, py = points[i]
                d = abs(dy * (px - ax) - dx * (py - ay))
                if d > worst:
                    worst, at = d, i
            worst /= length
        if worst > tolerance:
            keep[at] = True
            stack.append((a, at))
            stack.append((at, b))
    return [p for p, k in zip(points, keep) if k]


def area(points):
    return 0.5 * abs(sum(points[i - 1][0] * p[1] - p[0] * points[i - 1][1] for i, p in enumerate(points)))


def clip(points, axis, value, keep_low):
    """Sutherland-Hodgman against one half-plane; a point on the line is in."""
    if not points:
        return points
    out = []
    prev = points[-1]
    prev_in = prev[axis] <= value if keep_low else prev[axis] >= value
    for p in points:
        now_in = p[axis] <= value if keep_low else p[axis] >= value
        if now_in != prev_in:
            t = (value - prev[axis]) / (p[axis] - prev[axis])
            if axis == 0:
                out.append((value, prev[1] + (p[1] - prev[1]) * t))
            else:
                out.append((prev[0] + (p[0] - prev[0]) * t, value))
        if now_in:
            out.append(p)
        prev, prev_in = p, now_in
    return out


class Ring:
    __slots__ = ("points", "lo", "hi")

    def __init__(self, points):
        self.points = points
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        self.lo = (min(xs), min(ys))
        self.hi = (max(xs), max(ys))

    def clip(self, axis, value, keep_low):
        """The ring on one side of the line, or None."""
        if keep_low:
            if self.hi[axis] <= value:
                return self
            if self.lo[axis] >= value:
                return None
        else:
            if self.lo[axis] >= value:
                return self
            if self.hi[axis] <= value:
                return None
        points = clip(self.points, axis, value, keep_low)
        return Ring(points) if len(points) >= 3 else None


def clip_polygons(polygons, axis, value, keep_low):
    out = []
    for rings in polygons:
        outer = rings[0].clip(axis, value, keep_low)
        if outer is None:
            continue
        kept = [outer]
        for hole in rings[1:]:
            hole = hole.clip(axis, value, keep_low)
            if hole is not None:
                kept.append(hole)
        out.append(kept)
    return out


def polygons_area(polygons):
    return sum(max(0.0, area(rings[0].points) - sum(area(h.points) for h in rings[1:])) for rings in polygons)


def build(zones, cell, cols, rows):
    """The code of every cell, and the outlines of the shared cells."""
    grid = [[0] * cols for _ in range(rows)]
    shared = {}  # (row, col) -> (default zone, [(zone, [rings])])
    stack = [(0, cols, 0, rows, [(z, polys) for z, polys in enumerate(zones) if polys])]
    while stack:
        c0, c1, r0, r1, present = stack.pop()
        box = (c1 - c0) * (r1 - r0) * cell * cell
        areas = []
        for zone, polygons in present:
            a = polygons_area(polygons)
            if a > 1e-12:
                areas.append((a, zone, polygons))
        whole = next((zone for a, zone, _ in areas if a >= box * (1 - 1e-9)), None)
        if whole is not None or not areas:
            code = whole + 1 if whole is not None else 0
            for r in range(r0, r1):
                grid[r][c0:c1] = [code] * (c1 - c0)
            continue
        if c1 - c0 == 1 and r1 - r0 == 1:
            areas.sort(key=lambda item: -item[0])
            if len(areas) == 1:
                grid[r0][c0] = areas[0][1] + 1
            else:
                grid[r0][c0] = -1
                shared[(r0, c0)] = (areas[0][1], [(zone, polygons) for _, zone, polygons in areas[1:]])
            continue
        present = [(zone, polygons) for _, zone, polygons in areas]
        if c1 - c0 >= r1 - r0:
            mid = (c0 + c1) // 2
            value = -180.0 + mid * cell
            stack.append((c0, mid, r0, r1, [(z, clip_polygons(p, 0, value, True)) for z, p in present]))
            stack.append((mid, c1, r0, r1, [(z, clip_polygons(p, 0, value, False)) for z, p in present]))
        else:
            mid = (r0 + r1) // 2
            value = -90.0 + mid * cell
            stack.append((c0, c1, r0, mid, [(z, clip_polygons(p, 1, value, True)) for z, p in present]))
            stack.append((c0, c1, mid, r1, [(z, clip_polygons(p, 1, value, False)) for z, p in present]))
    return grid, shared


def varint(n, out):
    while True:
        byte = n & 0x7F
        n >>= 7
        if n:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return


def zigzag(n):
    return n * 2 if n >= 0 else -n * 2 - 1


def encode(names, grid, shared, cell, cols, rows):
    """The asset's bytes, and how many points and shared cells it keeps."""
    units = round(cell * QUANTUM)
    out = bytearray(MAGIC)
    varint(units, out)
    varint(len(names), out)
    for name in names:
        data = name.encode("utf-8")
        varint(len(data), out)
        out += data
    mixed = len(names) + 1
    cells = []
    points_kept = 0
    for r in range(rows):
        codes = []
        for c in range(cols):
            code = grid[r][c]
            if code == -1:
                default, parts = shared[(r, c)]
                body = bytearray()
                kept_parts = []
                west = -180.0 + c * cell
                south = -90.0 + r * cell
                for zone, polygons in parts:
                    rings = []
                    for ring in (ring for polygon in polygons for ring in polygon):
                        q = [(round((x - west) * QUANTUM), round((y - south) * QUANTUM)) for x, y in ring.points]
                        q = [p for i, p in enumerate(q) if p != q[i - 1]]
                        if len(q) >= 3 and area(q) > 0:
                            rings.append(q)
                    if rings:
                        kept_parts.append((zone, rings))
                if not kept_parts:
                    codes.append(default + 1)
                    continue
                varint(default + 1, body)
                varint(len(kept_parts), body)
                for zone, rings in kept_parts:
                    varint(zone, body)
                    varint(len(rings), body)
                    for q in rings:
                        varint(len(q), body)
                        px, py = 0, 0
                        for x, y in q:
                            varint(zigzag(x - px), body)
                            varint(zigzag(y - py), body)
                            px, py = x, y
                        points_kept += len(q)
                cells.append(body)
                codes.append(mixed)
            else:
                codes.append(code)
        runs = []
        for code in codes:
            if runs and runs[-1][1] == code and code != mixed:
                runs[-1][0] += 1
            else:
                runs.append([1, code])
        varint(len(runs), out)
        for length, code in runs:
            varint(length, out)
            varint(code, out)
    for body in cells:
        varint(len(body), out)
        out += body
    return bytes(out), points_kept, len(cells)


class Asset:
    """The asset read back, as the crate reads it: for --check."""

    def __init__(self, data):
        self.data = data
        self.at = 4
        units = self.varint()
        self.cell = units / QUANTUM
        self.names = []
        for _ in range(self.varint()):
            n = self.varint()
            self.names.append(data[self.at:self.at + n].decode("utf-8"))
            self.at += n
        self.cols, self.rows = round(360 / self.cell), round(180 / self.cell)
        self.grid = []
        count = 0
        for _ in range(self.rows):
            row = []
            for _ in range(self.varint()):
                length, code = self.varint(), self.varint()
                for _ in range(length):
                    if code == len(self.names) + 1:
                        row.append(-1 - count)
                        count += 1
                    else:
                        row.append(code)
            self.grid.append(row)
        self.cells = []
        for _ in range(count):
            n = self.varint()
            self.cells.append(self.at)
            self.at += n

    def varint(self):
        n, shift = 0, 0
        while True:
            byte = self.data[self.at]
            self.at += 1
            n |= (byte & 0x7F) << shift
            if byte < 0x80:
                return n
            shift += 7

    def svarint(self):
        n = self.varint()
        return (n >> 1) ^ -(n & 1)

    def zone_at(self, lon, lat):
        c = min(self.cols - 1, max(0, int((lon + 180) // self.cell)))
        r = min(self.rows - 1, max(0, int((lat + 90) // self.cell)))
        code = self.grid[r][c]
        if code >= 0:
            return self.names[code - 1] if code else None
        self.at = self.cells[-1 - code]
        x = (lon - (-180 + c * self.cell)) * QUANTUM
        y = (lat - (-90 + r * self.cell)) * QUANTUM
        default = self.varint()
        for _ in range(self.varint()):
            zone = self.varint()
            inside = False
            for _ in range(self.varint()):
                n = self.varint()
                pts = []
                px = py = 0
                for _ in range(n):
                    px += self.svarint()
                    py += self.svarint()
                    pts.append((px, py))
                for i, (bx, by) in enumerate(pts):
                    ax, ay = pts[i - 1]
                    if (ay > y) != (by > y) and x < ax + (y - ay) * (bx - ax) / (by - ay):
                        inside = not inside
            if inside:
                return self.names[zone]
        return self.names[default - 1] if default else None


def check(features, asset, seed=7):
    """Random places on land, by the full outlines and by the asset."""
    # Every edge of the full outlines by its band of latitude, a hundredth
    # of a degree, to tell the zone of a place, and by its square of a tenth,
    # for how far a place is from a border.
    bands, squares = {}, {}
    for zone, feature in enumerate(features):
        geometry = feature["geometry"]
        polygons = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
        for ring in (ring for polygon in polygons for ring in polygon):
            for (ax, ay), (bx, by) in zip(ring, ring[1:]):
                edge = (ax, ay, bx, by, zone)
                for band in range(math.floor(min(ay, by) * 100), math.floor(max(ay, by) * 100) + 1):
                    bands.setdefault(band, []).append(edge)
                for i in range(math.floor(min(ax, bx) * 10), math.floor(max(ax, bx) * 10) + 1):
                    for j in range(math.floor(min(ay, by) * 10), math.floor(max(ay, by) * 10) + 1):
                        squares.setdefault((i, j), []).append(edge)
    names = [f["properties"]["tzid"] for f in features]

    def zones_of(lon, lat):
        """The zones a place is in: two where the source has them overlap
        on purpose (Xinjiang, Abkhazia), as its expectedZoneOverlaps say."""
        inside = set()
        for ax, ay, bx, by, zone in bands.get(math.floor(lat * 100), ()):
            if (ay > lat) != (by > lat) and lon < ax + (lat - ay) * (bx - ax) / (by - ay):
                inside ^= {zone}
        return {names[zone] for zone in inside}

    def border_km(lon, lat):
        """How far the place is from the nearest edge, within 0.2°."""
        best = float("inf")
        k = math.cos(math.radians(lat))
        i0, j0 = math.floor(lon * 10), math.floor(lat * 10)
        for i in range(i0 - 2, i0 + 3):
            for j in range(j0 - 2, j0 + 3):
                for ax, ay, bx, by, _ in squares.get((i, j), ()):
                    ax, bx, px = ax * k, bx * k, lon * k
                    dx, dy = bx - ax, by - ay
                    t = 0.0 if dx == dy == 0 else max(0.0, min(1.0, ((px - ax) * dx + (lat - ay) * dy) / (dx * dx + dy * dy)))
                    best = min(best, math.hypot(px - (ax + t * dx), lat - (ay + t * dy)) * 111.2)
        return best

    rng = random.Random(seed)
    alps = (5.0, 17.0, 43.0, 48.5)
    # Edges of the borders in the Alps, to test places close to them.
    near = [e for (i, j), edges in squares.items() if alps[0] * 10 <= i < alps[1] * 10 and alps[2] * 10 <= j < alps[3] * 10 for e in edges]

    def by_a_border():
        ax, ay, bx, by, _ = rng.choice(near)
        t = rng.random()
        return ax + (bx - ax) * t + rng.uniform(-0.01, 0.01), ay + (by - ay) * t + rng.uniform(-0.01, 0.01)

    tests = [
        ("the Alps, 5° to 17° E, 43° to 48.5° N", lambda: (rng.uniform(5.0, 17.0), rng.uniform(43.0, 48.5))),
        ("within about a kilometre of a border or coast in the Alps", by_a_border),
        ("Europe, 10° W to 40° E, 35° to 60° N", lambda: (rng.uniform(-10.0, 40.0), rng.uniform(35.0, 60.0))),
        ("the world, 60° S to 75° N", lambda: (rng.uniform(-180.0, 180.0), rng.uniform(-60.0, 75.0))),
    ]
    for label, place in tests:
        tested = wrong = 0
        worst = 0.0
        while tested < 5000:
            lon, lat = place()
            truth = zones_of(lon, lat)
            if not truth:
                continue
            tested += 1
            got = asset.zone_at(lon, lat)
            if got not in truth:
                wrong += 1
                d = border_km(lon, lat)
                worst = max(worst, d)
                if d > 0.5:
                    print(f"  wrong {d:.2f} km from a border at {lon:.4f} {lat:.4f}: {got} for {' or '.join(sorted(truth))}")
        print(f"{label}: {tested} places on land, {wrong} given another zone, the farthest {worst:.2f} km from a border")


def main():
    global TOLERANCE
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    source, target = args[0], args[1]
    for a in sys.argv[1:]:
        if a.startswith("--tolerance="):
            TOLERANCE = float(a.split("=", 1)[1])
    with open(source) as f:
        collection = json.load(f)
    features = sorted(collection["features"], key=lambda f: f["properties"]["tzid"])
    names = [f["properties"]["tzid"] for f in features]
    if len(set(names)) != len(names):
        raise SystemExit("a zone twice")
    zones = []
    points_in = points_simplified = 0
    for feature in features:
        geometry = feature["geometry"]
        polygons = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
        kept = []
        for polygon in polygons:
            rings = []
            for i, ring in enumerate(polygon):
                points = [tuple(p) for p in ring]
                if points[0] == points[-1]:
                    points.pop()
                points_in += len(points)
                simplified = simplify(points, TOLERANCE)
                # A ring the tolerance would close up keeps its every point.
                if len(simplified) < 3:
                    simplified = points
                if len(simplified) < 3:
                    continue
                points_simplified += len(simplified)
                if i == 0 or rings:
                    rings.append(Ring(simplified))
            if rings:
                kept.append(rings)
        zones.append(kept)
    cols, rows = round(360 / CELL), round(180 / CELL)
    grid, shared = build(zones, CELL, cols, rows)
    data, points_kept, cells_kept = encode(names, grid, shared, CELL, cols, rows)
    with open(target, "wb") as out:
        out.write(data)
    flat = [code for row in grid for code in row]
    print(f"{len(names)} zones; cells of {CELL}°: {flat.count(0)} without a zone, "
          f"{sum(1 for c in flat if c > 0)} of one zone, {cells_kept} shared")
    print(f"points: {points_in} in the outlines, {points_simplified} simplified by {TOLERANCE}°, {points_kept} kept")
    print("size", os.path.getsize(target), "bytes")
    if "--check" in sys.argv:
        check(features, Asset(data))


main()
