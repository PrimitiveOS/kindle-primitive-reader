#!/usr/bin/env python3
"""Create the original, uncompressed illustrated MOBI used for Reader's built-in-link proof."""
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


def publication():
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
    struct.pack_into(">HHIHH", hdr, 0, 1, 0, len(html), len(records), 4096)
    hdr[16:20] = b"MOBI"
    for at, value in [(20, 248), (24, 2), (28, 65001), (36, 6), (108, len(records) + 1)]:
        struct.pack_into(">I", hdr, at, value)
    chunks = [hdr, *records, bitmap()]
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
    args.parse_args().output.write_bytes(publication())
