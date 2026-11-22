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


def variable(value):
    encoded = [0x80 | (value & 0x7f)]
    value >>= 7
    while value:
        encoded.append(value & 0x7f)
        value >>= 7
    return bytes(reversed(encoded))


def navigation(html):
    # Tag 1 is a source byte offset, tag 3 identifies a CNCX title, and
    # tags 4, 21, 22 and 23 describe level, parent and child relationships.
    rows = [
        ("Coastal field notes", b"<h1", 0, None, (1, 2)),
        ("Elevation profile", b"<p><img", 1, 0, None),
        ("Survey notes", b"<p>Survey 1.", 1, 0, None),
        ("Summit notes", b"<h2>", 0, None, None),
    ]
    schema = b"".join(bytes([tag, 1, 1 << bit, 0]) for bit, tag in enumerate((1, 3, 4, 21, 22, 23)))
    schema += bytes([0, 0, 0, 1])
    meta = bytearray(192)
    data = bytearray(192)
    for header in (meta, data):
        struct.pack_into(">4sI", header, 0, b"INDX", 192)
    for at, value in [(24, 1), (28, 65001), (32, 0xffffffff), (36, len(rows)), (52, 1)]:
        struct.pack_into(">I", meta, at, value)
    meta.extend(struct.pack(">4sII", b"TAGX", len(schema) + 12, 1) + schema)
    names, offsets = bytearray(), []
    for index, (title, marker, depth, parent, children) in enumerate(rows):
        label = f"{index:03x}".encode()
        offsets.append(len(data))
        fields = [html.index(marker), len(names), depth]
        mask = 7
        if parent is not None:
            mask |= 8
            fields.append(parent)
        if children:
            mask |= 48
            fields.extend(children)
        data.extend(bytes([len(label)]) + label + bytes([mask]) + b"".join(variable(n) for n in fields))
        text = title.encode()
        names.extend(variable(len(text)) + text)
    struct.pack_into(">II", data, 20, len(data), len(rows))
    data.extend(b"IDXT" + b"".join(struct.pack(">H", n) for n in offsets))
    # A metadata IDXT entry identifies the final key and row count for this segment.
    metadata_key = len(meta)
    meta.extend(bytes([3]) + b"003" + struct.pack(">H", len(rows)))
    struct.pack_into(">I", meta, 20, len(meta))
    meta.extend(b"IDXT" + struct.pack(">H", metadata_key))
    return meta, data, names


def publication(huff=False, styled=False, contents=False):
    opening = ('<html><body><h1>Coastal field notes</h1>'
               '<p>A walking guide to the headland.</p>'
               '<p><img recindex="00001" width="440" height="176" alt="Profile of the headland"/></p>'
               '<p>The profile shows the climb from the shore to the high point.</p>'
               '<p><a filepos="0000000000">Read the summit notes</a></p>')
    if styled:
        opening = opening.replace("<h1>", '<h1 align="center">').replace(
            "<p>A walking guide to the headland.</p>",
            '<p align="center"><font size="+1">A walking guide to the headland.</font></p>',
        ).replace("Read the summit notes</a>", '<font size="4">Read the summit notes</font></a>')
    body = "".join(f"<p>Survey {n}. The path follows the sheltered side of the ridge. "
                   "Pause at the clearing to compare the view with the elevation profile.</p>"
                   for n in range(1, 46))
    end = "<h2>Summit notes</h2><p>The highest point overlooks the inlet.</p></body></html>"
    if styled:
        # Labels of similar length also serve as reproducible selection targets.
        scale = [("0", "Shore station"), ("+1", "Dune station"),
                 ("-1", "Cove station"), ("-4", "Reef station"),
                 ("+20", "Tide station"), ("5extra", "Gale station"),
                 ("invalid", "Dock station")]
        samples = "".join(f'<p><font size="{size}">{label}</font></p>' for size, label in scale)
        end = end.replace("</body>", samples + "</body>")
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
    if contents:
        struct.pack_into(">I", hdr, 244, len(chunks))
        chunks.extend(navigation(html))
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
    args.add_argument("--styled", action="store_true", help="include legacy publisher alignment and font sizes")
    args.add_argument("--contents", action="store_true", help="include a nested publisher navigation index")
    options = args.parse_args()
    options.output.write_bytes(publication(options.huff, options.styled, options.contents))
