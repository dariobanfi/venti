#!/usr/bin/env python3
"""Draws the pictures of the seven made-up sample pilots.

Each is a made-up illustration, no face and no photograph: a paraglider's
canopy and its pilot over a ridge, on a sky of the pilot's own colour.
Python's standard library only; the same files on every run.

    python3 crates/server/assets/sample-pilots/make.py
"""

import math
import pathlib
import struct
import zlib

SIDE = 192

# handle: (sky, ridge, canopy, tilt of the canopy in degrees)
PILOTS = {
    "sophie": ((122, 178, 214), (62, 96, 82), (232, 92, 60), -12),
    "felix": ((240, 190, 120), (98, 70, 92), (40, 120, 200), 8),
    "kathi": ((150, 200, 170), (54, 84, 110), (250, 210, 70), -4),
    "giulia": ((236, 150, 140), (80, 60, 70), (250, 250, 240), 14),
    "reto": ((170, 160, 220), (60, 70, 60), (220, 40, 60), -16),
    "camille": ((120, 200, 210), (40, 90, 110), (250, 140, 40), 4),
    "nejc": ((210, 220, 140), (70, 90, 50), (120, 60, 180), -8),
}


def ridge_height(x, seed):
    """The top of the ridge at column x, in pixels from the top."""
    t = x / SIDE
    return SIDE * (0.70 + 0.08 * math.sin(6.0 * t + seed) + 0.05 * math.sin(13.0 * t + 2 * seed))


def draw(sky, ridge, canopy, tilt, seed):
    rows = []
    centre_x, centre_y = SIDE * 0.5, SIDE * 0.36
    # The canopy is the upper half of a flat ellipse, as a wing seen from
    # behind: a by b pixels, a band of it.
    a, b, band = SIDE * 0.36, SIDE * 0.13, 0.62
    angle = math.radians(tilt)
    cos_a, sin_a = math.cos(angle), math.sin(angle)

    def turned(x, y):
        return centre_x + x * cos_a - y * sin_a, centre_y + x * sin_a + y * cos_a

    # The pilot hangs under the canopy's middle, turned with it.
    pilot = turned(0.0, SIDE * 0.24)
    # The lines run from the canopy's lower edge, at four places.
    inner = math.sqrt(band)
    tips = [
        turned(side * a * inner * along, -b * inner * math.sqrt(1 - along**2))
        for side in (-1, 1)
        for along in (0.9, 0.4)
    ]
    for py in range(SIDE):
        row = bytearray()
        for px in range(SIDE):
            colour = sky
            if py >= ridge_height(px, seed):
                colour = ridge
            dx, dy = px - centre_x, py - centre_y
            rx, ry = dx * cos_a + dy * sin_a, -dx * sin_a + dy * cos_a
            # The lines from the canopy to the pilot, then the canopy.
            for tx, ty in tips:
                if near_segment(px, py, tx, ty, pilot[0], pilot[1], 1.1):
                    colour = (40, 40, 48)
            inside = (rx / a) ** 2 + (ry / b) ** 2
            if ry <= 0 and band <= inside <= 1.0:
                colour = canopy
            if math.hypot(px - pilot[0], py - pilot[1]) <= SIDE * 0.035:
                colour = (40, 40, 48)
            row += bytes(colour)
        rows.append(bytes(row))
    return rows


def near_segment(px, py, ax, ay, bx, by, width):
    vx, vy = bx - ax, by - ay
    length = vx * vx + vy * vy
    t = max(0.0, min(1.0, ((px - ax) * vx + (py - ay) * vy) / length))
    return math.hypot(px - (ax + t * vx), py - (ay + t * vy)) <= width


def png(rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", SIDE, SIDE, 8, 2, 0, 0, 0)
    raw = b"".join(b"\x00" + row for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    folder = pathlib.Path(__file__).resolve().parent
    for seed, (handle, (sky, ridge, canopy, tilt)) in enumerate(PILOTS.items()):
        path = folder / f"{handle}.png"
        path.write_bytes(png(draw(sky, ridge, canopy, tilt, seed)))
        print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
