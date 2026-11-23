# Makes assets/europe.geojson, the outlines of the countries of Europe,
# from Natural Earth's "Admin 0 - Countries" at 1:10m (public domain): see
# the manifest's [[asset]] for the release and where it is downloaded.
#
# Not run by the build or the tests. Needs Python 3 alone, nothing to
# install. Takes the folder that ne_10m_admin_0_countries.zip was unpacked
# into and the file to write:
#
#   python3 -I tools/build_countries.py <unpacked folder> assets/europe.geojson
#
# Each country's rings are clipped to a box around Europe, simplified by
# Douglas-Peucker with a tolerance of 0.001 degrees (about 100 m), and
# rounded to five decimals; a ring left with less than about a square
# kilometre is dropped. The rings are simplified each on its own, so two
# neighbours' borders may part or cross by up to the tolerance.
import json
import os
import struct
import sys

WEST, SOUTH, EAST, NORTH = -32.0, 34.0, 45.0, 72.0
TOLERANCE = 0.001
SMALLEST_RING = 0.0001  # square degrees, about 1 km² at 47° N


def read_dbf(path):
    with open(path, "rb") as f:
        data = f.read()
    count, header_size, record_size = struct.unpack("<IHH", data[4:12])
    fields = []
    offset = 32
    while data[offset] != 0x0D:
        name = data[offset:offset + 11].split(b"\0")[0].decode("ascii")
        size = data[offset + 16]
        fields.append((name, size))
        offset += 32
    records = []
    for i in range(count):
        start = header_size + i * record_size + 1  # past the deletion flag
        row = {}
        for name, size in fields:
            row[name] = data[start:start + size].decode("utf-8", "replace").strip(" \0")
            start += size
        records.append(row)
    return records


def read_shp(path):
    """Each shape as a list of rings, each a list of (lon, lat)."""
    with open(path, "rb") as f:
        data = f.read()
    shapes = []
    offset = 100
    while offset < len(data):
        _, length = struct.unpack(">II", data[offset:offset + 8])
        body = data[offset + 8:offset + 8 + 2 * length]
        offset += 8 + 2 * length
        kind = struct.unpack("<i", body[:4])[0]
        if kind == 0:
            shapes.append([])
            continue
        if kind != 5:
            raise SystemExit(f"a shape of type {kind}, not a polygon")
        parts, points = struct.unpack("<ii", body[36:44])
        starts = list(struct.unpack(f"<{parts}i", body[44:44 + 4 * parts])) + [points]
        base = 44 + 4 * parts
        xy = struct.unpack(f"<{2 * points}d", body[base:base + 16 * points])
        rings = []
        for a, b in zip(starts, starts[1:]):
            rings.append([(xy[2 * i], xy[2 * i + 1]) for i in range(a, b)])
        shapes.append(rings)
    return shapes


def clip(ring):
    """Sutherland-Hodgman against the box; the ring stays closed."""
    def edge(points, inside, cross):
        out = []
        for i, p in enumerate(points):
            q = points[i - 1]
            if inside(p):
                if not inside(q):
                    out.append(cross(q, p))
                out.append(p)
            elif inside(q):
                out.append(cross(q, p))
        return out

    def at_x(x):
        return lambda q, p: (x, q[1] + (p[1] - q[1]) * (x - q[0]) / (p[0] - q[0]))

    def at_y(y):
        return lambda q, p: (q[0] + (p[0] - q[0]) * (y - q[1]) / (p[1] - q[1]), y)

    points = ring[:-1] if ring[0] == ring[-1] else ring
    for inside, cross in (
        (lambda p: p[0] >= WEST, at_x(WEST)),
        (lambda p: p[0] <= EAST, at_x(EAST)),
        (lambda p: p[1] >= SOUTH, at_y(SOUTH)),
        (lambda p: p[1] <= NORTH, at_y(NORTH)),
    ):
        if not points:
            break
        points = edge(points, inside, cross)
    return points


def simplify(points):
    """Douglas-Peucker on a closed ring, without recursion."""
    n = len(points)
    if n < 4:
        return points
    # Split the ring at its first point and the point furthest from it.
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


def main():
    folder, target = sys.argv[1], sys.argv[2]
    base = os.path.join(folder, "ne_10m_admin_0_countries")
    records = read_dbf(base + ".dbf")
    shapes = read_shp(base + ".shp")
    with open(os.path.join(folder, "ne_10m_admin_0_countries.VERSION.txt")) as f:
        version = f.read().strip()
    features = []
    for row, rings in zip(records, shapes):
        code = row.get("ISO_A2_EH") or row.get("ISO_A2")
        if row.get("CONTINENT") != "Europe" and code != "CY":
            continue
        if not code or code == "-99" or len(code) != 2:
            print("left out, no code:", row.get("NAME"))
            continue
        polygons = []
        # A country smaller than the smallest ring kept (the Vatican) keeps
        # its largest. Holes are all kept: they are enclaves, San Marino
        # and the Vatican in Italy.
        smallest = min(SMALLEST_RING, max(abs(area(ring)) for ring in rings) * 0.5)
        for ring in rings:
            # Shapefiles draw an outer ring clockwise and a hole the other
            # way round: a ring of positive area (anticlockwise) is a hole.
            hole = area(ring) > 0
            clipped = clip(ring)
            simplified = simplify(clipped)
            # A ring the tolerance would close up keeps its every point.
            if len(simplified) < 3:
                simplified = clipped
            points = [(round(x, 5), round(y, 5)) for x, y in simplified]
            points = [p for i, p in enumerate(points) if p != points[i - 1]]
            if len(points) < 3 or (not hole and abs(area(points)) < smallest):
                continue
            closed = [list(p) for p in points] + [list(points[0])]
            if hole and polygons:
                polygons[-1].append(closed)
            elif not hole:
                polygons.append([closed])
        if not polygons:
            print("left out, nothing in the box:", row.get("NAME"))
            continue
        features.append({
            "type": "Feature",
            "properties": {"code": code, "name": row.get("NAME")},
            "geometry": {"type": "MultiPolygon", "coordinates": polygons},
        })
    features.sort(key=lambda f: f["properties"]["code"])
    codes = [f["properties"]["code"] for f in features]
    if len(set(codes)) != len(codes):
        raise SystemExit(f"a code twice: {codes}")
    collection = {
        "type": "FeatureCollection",
        "properties": {"source": f"Natural Earth, Admin 0 - Countries 1:10m, version {version}, public domain"},
        "features": features,
    }
    with open(target, "w") as out:
        json.dump(collection, out, separators=(",", ":"), ensure_ascii=False)
    print(len(features), "countries:", " ".join(codes))
    print("version", version, "size", os.path.getsize(target), "bytes")


main()
