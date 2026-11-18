#!/usr/bin/env python3
"""Draws the app icon: a white paraglider on the accent violet, with the
pilot as a dot in the colour a climb has on a track.

    python3 tools/app-icon/make_icon.py apps/ios/Venti/Assets.xcassets/AppIcon.appiconset/icon-1024.png

Needs Pillow and NumPy. The icon is square and opaque; iOS rounds its corners.
"""

import sys

import numpy as np
from PIL import Image, ImageDraw

SIZE = 1024
SCALE = 4  # Drawn large and reduced, which smooths every edge.

TOP = (0x8A, 0x7B, 0xF2)
BOTTOM = (0x4B, 0x38, 0xBE)
WING = (255, 255, 255)
PILOT = (0xEB, 0x68, 0x34)


def background(side: int) -> Image.Image:
    """The accent violet, lighter at the top left than at the bottom right."""
    y, x = np.mgrid[0:side, 0:side] / (side - 1)
    blend = (0.8 * y + 0.2 * x)[..., None]
    pixels = np.array(TOP) * (1 - blend) + np.array(BOTTOM) * blend
    return Image.fromarray(pixels.astype(np.uint8), "RGB")


def upper_edge(t: float) -> float:
    return 286 + 124 * t * t


def lower_edge(t: float) -> float:
    return 352 + 58 * t * t


def canopy(centre: float, half_width: float) -> list[tuple[float, float]]:
    """A crescent: its upper and lower edges are parabolas that meet at the tips."""
    steps = [i / 100 for i in range(-100, 101)]
    upper = [(centre + t * half_width, upper_edge(t)) for t in steps]
    lower = [(centre + t * half_width, lower_edge(t)) for t in reversed(steps)]
    return upper + lower


def main(path: str) -> None:
    side = SIZE * SCALE
    image = background(side)
    draw = ImageDraw.Draw(image, "RGBA")

    def scaled(points):
        return [(x * SCALE, y * SCALE) for x, y in points]

    centre, half_width = 512, 392
    pilot = (centre, 760)

    # The lines, drawn first so the canopy and the pilot cover their ends.
    for t in (-0.86, -0.42, 0.42, 0.86):
        attachment = (centre + t * half_width, lower_edge(t) - 6)
        draw.line(scaled([attachment, pilot]), fill=WING + (170,), width=7 * SCALE)

    draw.polygon(scaled(canopy(centre, half_width)), fill=WING + (255,))

    radius = 50
    box = [(pilot[0] - radius, pilot[1] - radius), (pilot[0] + radius, pilot[1] + radius)]
    draw.ellipse([tuple(v * SCALE for v in corner) for corner in box], fill=PILOT + (255,))

    image.resize((SIZE, SIZE), Image.LANCZOS).save(path, optimize=True)


if __name__ == "__main__":
    main(sys.argv[1])
