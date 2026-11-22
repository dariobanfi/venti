# Makes assets/france.geojson, the outline of metropolitan France (the
# mainland, its islands and Corsica), from IGN's ADMIN EXPRESS COG, edition
# 2026 (Licence Ouverte / Open Licence): see the manifest's [[asset]].
#
# Not run by the build or the tests. Needs Python 3 and nothing else. The
# thirteen metropolitan regions are fetched beforehand from the
# Géoplateforme's WFS, one file each, into the folder named as the first
# argument (region_<code>.json, code_insee 11 24 27 28 32 44 52 53 75 76 84
# 93 94):
#
#   https://data.geopf.fr/wfs/ows?SERVICE=WFS&VERSION=2.0.0&REQUEST=GetFeature
#     &TYPENAMES=ADMINEXPRESS-COG.2026:region&outputFormat=application/json
#     &srsName=EPSG:4326&CQL_FILTER=code_insee%3D%27<code>%27
#
# What it does: joins the regions into one outline by dropping every piece
# of boundary that two regions share (the regions are cut from one mesh, so
# a shared piece has the same positions in both), simplifies each ring by
# Douglas-Peucker in Lambert-93 metres, at 5 m where the Alps' neighbours
# are (east of 5.5° E between 43.6° and 47.7° N: Switzerland, Italy,
# Monaco) and at 15 m elsewhere, drops islets under a hectare, and rounds
# to six decimals. Writes the outline to the path named as the second
# argument and prints what it found.

import glob, json, math, sys

SHARP = (5.5, 43.6, 47.7)   # west, south, north of the part kept at 5 m
TOL_SHARP, TOL_ELSEWHERE = 5.0, 15.0
MIN_ISLAND_M2 = 10_000.0
CODES = "11 24 27 28 32 44 52 53 75 76 84 93 94".split()

# Lambert-93 (EPSG:2154) forward, on GRS80.
A, F = 6378137.0, 1 / 298.257222101
E = math.sqrt(2 * F - F * F)
def _m(phi): return math.cos(phi) / math.sqrt(1 - (E * math.sin(phi)) ** 2)
def _t(phi):
    s = math.sin(phi)
    return math.tan(math.pi / 4 - phi / 2) / ((1 - E * s) / (1 + E * s)) ** (E / 2)
P1, P2, P0, L0 = map(math.radians, (49.0, 44.0, 46.5, 3.0))
N = (math.log(_m(P1)) - math.log(_m(P2))) / (math.log(_t(P1)) - math.log(_t(P2)))
FF = _m(P1) / (N * _t(P1) ** N)
R0 = A * FF * _t(P0) ** N
def lambert(lon, lat):
    r = A * FF * _t(math.radians(lat)) ** N
    th = N * (math.radians(lon) - L0)
    return 700000.0 + r * math.sin(th), 6600000.0 + R0 - r * math.cos(th)

def rings_of(path):
    data = json.load(open(path))
    assert len(data["features"]) == 1, path
    g = data["features"][0]["geometry"]
    polys = g["coordinates"] if g["type"] == "MultiPolygon" else [g["coordinates"]]
    for poly in polys:
        for ring in poly:
            ring = [(p[0], p[1]) for p in ring]
            if ring[0] == ring[-1]: ring.pop()
            yield ring

def area(xy):
    s = 0.0
    for i in range(len(xy)):
        x0, y0 = xy[i - 1]; x1, y1 = xy[i]
        s += x0 * y1 - x1 * y0
    return s / 2

def simplify_open(pts, xy, tol_of):
    keep = [False] * len(pts); keep[0] = keep[-1] = True
    stack = [(0, len(pts) - 1)]
    while stack:
        a, b = stack.pop()
        if b - a < 2: continue
        ax, ay = xy[a]; bx, by = xy[b]
        dx, dy = bx - ax, by - ay
        length = math.hypot(dx, dy)
        worst, at = -1.0, a
        for i in range(a + 1, b):
            px, py = xy[i]
            if length == 0: d = math.hypot(px - ax, py - ay)
            else:
                t = ((px - ax) * dx + (py - ay) * dy) / (length * length)
                t = 0 if t < 0 else 1 if t > 1 else t
                d = math.hypot(px - ax - t * dx, py - ay - t * dy)
            d = d / tol_of(pts[i])
            if d > worst: worst, at = d, i
        if worst > 1.0:
            keep[at] = True
            stack.append((a, at)); stack.append((at, b))
    return [i for i, k in enumerate(keep) if k]

def tol_of(p):
    west, south, north = SHARP
    return TOL_SHARP if p[0] >= west and south <= p[1] <= north else TOL_ELSEWHERE

def simplify_ring(ring):
    xy = [lambert(*p) for p in ring]
    # Split the ring at its first position and the one furthest from it.
    far = max(range(len(ring)), key=lambda i: (xy[i][0] - xy[0][0]) ** 2 + (xy[i][1] - xy[0][1]) ** 2)
    if far == 0: return []
    first = simplify_open(ring[: far + 1], xy[: far + 1], tol_of)
    second = simplify_open(ring[far:] + ring[:1], xy[far:] + xy[:1], tol_of)
    out = [ring[i] for i in first] + [(ring[far:] + ring[:1])[i] for i in second[1:-1]]
    return out

def main(folder, out_path):
    rings = []
    for code in CODES:
        rings.extend(rings_of(f"{folder}/region_{code}.json"))
    points = sum(len(r) for r in rings)
    print("rings of the regions", len(rings), "positions", points)
    # Every piece of boundary, by its two ends; a piece two rings have is inside.
    seen = {}
    for ring in rings:
        for i in range(len(ring)):
            a, b = ring[i - 1], ring[i]
            key = (a, b) if a < b else (b, a)
            seen[key] = seen.get(key, 0) + 1
    assert max(seen.values()) <= 2
    shared = sum(1 for n in seen.values() if n == 2)
    print("pieces", len(seen), "shared by two regions", shared)
    after = {}
    for ring in rings:
        for i in range(len(ring)):
            a, b = ring[i - 1], ring[i]
            key = (a, b) if a < b else (b, a)
            if seen[key] == 1:
                after.setdefault(a, []).append(b)
    joined = []
    while after:
        start = next(iter(after))
        ring = [start]; here = start
        while True:
            nexts = after[here]
            there = nexts.pop()
            if not nexts: del after[here]
            if there == start: break
            ring.append(there); here = there
        joined.append(ring)
    print("rings of the outline", len(joined), "positions", sum(len(r) for r in joined))
    kept, dropped, holes = [], 0, 0
    for ring in joined:
        a = area([lambert(*p) for p in ring])
        if abs(a) < MIN_ISLAND_M2:
            dropped += 1; continue
        s = simplify_ring(ring)
        if len(s) < 3:
            dropped += 1; continue
        kept.append((a, s))
    # The largest ring is the mainland; a ring that turns the other way is a hole.
    kept.sort(key=lambda r: -abs(r[0]))
    sign = 1 if kept[0][0] > 0 else -1
    outer = [(a, r) for a, r in kept if a * sign > 0]
    inner = [(a, r) for a, r in kept if a * sign < 0]
    print("kept", len(kept), "dropped as under a hectare or to nothing", dropped, "holes", len(inner))
    for a, r in kept[:6]: print("  ring of km2", round(abs(a) / 1e6, 1), "positions", len(r))
    for a, r in inner: print("  hole of km2", round(abs(a) / 1e6, 2), "at", r[0])
    def inside(p, ring):
        x, y = p; c = False
        for i in range(len(ring)):
            (x0, y0), (x1, y1) = ring[i - 1], ring[i]
            if (y0 > y) != (y1 > y) and x < x0 + (y - y0) / (y1 - y0) * (x1 - x0): c = not c
        return c
    polys = [[r] for _, r in outer]
    for _, hole in inner:
        owner = next(i for i, (_, r) in enumerate(outer) if inside(hole[0], r))
        polys[owner].append(hole)
    def closed(r):
        r = [[round(x, 6), round(y, 6)] for x, y in r]
        return r + [r[0]]
    coords = [[closed(r) for r in poly] for poly in polys]
    n = sum(len(r) for poly in coords for r in poly)
    lons = [p[0] for poly in coords for r in poly for p in r]; lats = [p[1] for poly in coords for r in poly for p in r]
    print("positions written", n, "box", min(lons), min(lats), max(lons), max(lats))
    print("area km2 (Lambert-93)", round(sum(abs(a) for a, _ in outer) / 1e6 - sum(abs(a) for a, _ in inner) / 1e6, 1))
    feature = {"type": "Feature", "properties": {"name": "France métropolitaine", "source": "IGN ADMIN EXPRESS COG 2026, Licence Ouverte / Open Licence"}, "geometry": {"type": "MultiPolygon", "coordinates": coords}}
    json.dump({"type": "FeatureCollection", "features": [feature]}, open(out_path, "w"), separators=(",", ":"), ensure_ascii=False)

main(sys.argv[1], sys.argv[2])
