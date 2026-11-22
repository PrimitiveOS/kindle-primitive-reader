#!/usr/bin/env python3
"""Create the illustrated MOBI used for Reader's image, link and compression checks."""
import argparse
from pathlib import Path
import struct
import zlib


def bitmap():
    width, height = 440, 176
    line = [(0, 143), (75, 109), (125, 126), (195, 65), (235, 82), (307, 24), (375, 64), (439, 49)]
    base = bytearray([255]) * (width * height)
    for (left, y0), (right, y1) in zip(line, line[1:]):
        for x in range(left, right + 1):
            y = round(y0 + (y1 - y0) * (x - left) / (right - left))
            for py in range(y, height):
                base[py * width + x] = 40 if py < y + 3 else 178 if py > y + 25 else 224

    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data)))

    rows = b"".join(b"\0" + base[y * width:(y + 1) * width] for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", width, height, 8, 0, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def byte_dictionary():
    """One canonical eight-bit code per literal byte, with the highest code first."""
    prefix_offset = 24
    bounds_offset = prefix_offset + 256 * 4
    little_prefix = bounds_offset + 32 * 8
    little_bounds = little_prefix + 256 * 4
    huff = bytearray(little_bounds + 32 * 8)
    struct.pack_into(">4s5I", huff, 0, b"HUFF", 24, prefix_offset, bounds_offset, little_prefix, little_bounds)
    for i in range(256):
        struct.pack_into(">I", huff, prefix_offset + 4 * i, 0xff88)
        struct.pack_into("<I", huff, little_prefix + 4 * i, 0xff88)
    struct.pack_into("<II", huff, little_bounds + 7 * 8, 0, 255)
    struct.pack_into(">II", huff, bounds_offset + 7 * 8, 0, 255)
    cdic = bytearray(struct.pack(">4s3I", b"CDIC", 16, 256, 8))
    cdic.extend(b"".join(struct.pack(">H", 512 + 3 * i) for i in range(256)))
    cdic.extend(b"".join(struct.pack(">HB", 0x8001, 255 - i) for i in range(256)))
    return huff, cdic


def publication(huff=False):
    opening = ('<html><body><h1>Coastal field notes</h1>'
               '<p>A walking guide to the headland.</p>'
               '<p><img recindex="00001" width="440" height="176" alt="Profile of the headland"/></p>'
               '<p>The profile shows the climb from the shore to the high point.</p>'
               '<p><a filepos="0000000000">Read the summit notes</a></p>')
    body = "".join(f"<p>Survey {n}. The path follows the sheltered side of the ridge. "
                   "Pause at the clearing to compare the view with the elevation profile.</p>"
                   for n in range(1, 46))
    end = "<h2>Summit notes</h2><p>The highest point overlooks the inlet.</p></body></html>"
    html = (opening.replace("0000000000", f"{len(opening + body):010}") + body + end).encode()
    records = [html[i:i + 4096] for i in range(0, len(html), 4096)]
    hdr = bytearray(264)
    struct.pack_into(">HHIHH", hdr, 0, 17480 if huff else 1, 0, len(html), len(records), 4096)
    hdr[16:20] = b"MOBI"
    for at, value in [(20, 248), (24, 2), (28, 65001), (36, 6), (80, len(records) + 1), (108, len(records) + 1)]:
        struct.pack_into(">I", hdr, at, value)
    # Unused dictionary and navigation indexes use the no-record sentinel.
    for offset in (*range(40, 80, 4), 244, 256):
        struct.pack_into(">I", hdr, offset, 0xffffffff)
    chunks = [hdr, *records, bitmap()]
    if huff:
        struct.pack_into(">II", hdr, 112, len(chunks), 2)
        chunks.extend(byte_dictionary())
    pdb = bytearray(78 + 8 * len(chunks))
    pdb[:19] = b"Coastal field notes"
    pdb[60:68] = b"BOOKMOBI"
    struct.pack_into(">H", pdb, 76, len(chunks))
    for number, chunk in enumerate(chunks):
        struct.pack_into(">I", pdb, 78 + number * 8, len(pdb))
        pdb.extend(chunk)
    return pdb


if __name__ == "__main__":
    args = argparse.ArgumentParser()
    args.add_argument("output", type=Path)
    args.add_argument("--huff", action="store_true", help="write HUFF/CDIC text records")
    options = args.parse_args()
    options.output.write_bytes(publication(options.huff))
