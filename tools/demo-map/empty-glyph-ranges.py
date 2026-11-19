"""Writes a glyph file without glyphs for every range that has no file yet.

Usage: empty-glyph-ranges.py <font folder> <range that has real glyphs>...

The map engine asks for the range of every character a label holds. A file
that says "this font has no glyphs here" answers the question; a missing
file leaves it open, and on the phones the map tile then never finishes.
"""
import pathlib
import sys

folder, real = pathlib.Path(sys.argv[1]), set(sys.argv[2:])
for start in range(0, 65536, 256):
    name = f"{start}-{start + 255}"
    if name in real:
        continue
    # Field 1, a font stack, holding field 1, its name, and field 2, its
    # range: what the fonts' source publishes for a range it does not cover.
    font, span = b"Sans Regular", name.encode()
    stack = bytes([0x0A, len(font)]) + font + bytes([0x12, len(span)]) + span
    (folder / f"{name}.pbf").write_bytes(bytes([0x0A, len(stack)]) + stack)
