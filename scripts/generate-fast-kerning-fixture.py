#!/usr/bin/env python3
"""Build the authored pair-spacing book and renamed SIL OFL test font."""
from pathlib import Path
import struct
import zipfile


ROOT = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


def tables(font):
    count = struct.unpack_from(">H", font, 4)[0]
    for index in range(count):
        tag, _, offset, size = struct.unpack_from(">4sIII", font, 12 + 16 * index)
        yield tag, font[offset:offset + size]


def glyph(cmap, character):
    code = ord(character)
    count = struct.unpack_from(">H", cmap, 2)[0]
    records = [struct.unpack_from(">HHI", cmap, 4 + 8 * i) for i in range(count)]
    for platform, encoding, table in records:
        if platform != 3 or encoding != 1 or struct.unpack_from(">H", cmap, table)[0] != 4:
            continue
        count = struct.unpack_from(">H", cmap, table + 6)[0] // 2
        for index in range(count):
            end = struct.unpack_from(">H", cmap, table + 14 + 2 * index)[0]
            start = struct.unpack_from(">H", cmap, table + 16 + 2 * (count + index))[0]
            if not start <= code <= end:
                continue
            delta = struct.unpack_from(">h", cmap, table + 16 + 2 * (2 * count + index))[0]
            location = table + 16 + 2 * (3 * count + index)
            relative = struct.unpack_from(">H", cmap, location)[0]
            if relative:
                base = struct.unpack_from(">H", cmap, location + relative + 2 * (code - start))[0]
                assert base
            else:
                base = code
            return (base + delta) & 0xFFFF
    raise ValueError(character)


def name_table():
    names = {
        0: "Copyright 2001 The Silkscreen Project Authors",
        1: "Harbor Pair Test",
        2: "Regular",
        3: "Harbor Pair Test Regular 1.0",
        4: "Harbor Pair Test Regular",
        5: "Version 1.0",
        6: "HarborPairTest-Regular",
        13: "SIL Open Font License, Version 1.1",
        14: "https://scripts.sil.org/OFL",
    }
    records, strings = bytearray(), bytearray()
    for identity, name in names.items():
        encoded = name.encode("utf-16-be")
        records.extend(struct.pack(">6H", 3, 1, 0x409, identity, len(encoded), len(strings)))
        strings.extend(encoded)
    return struct.pack(">3H", 0, len(names), 6 + len(records)) + records + strings


def checksum(table):
    padded = table + bytes(-len(table) % 4)
    return sum(value for (value,) in struct.iter_unpack(">I", padded)) & 0xFFFFFFFF


def proof_font():
    original = (ROOT / "fonts" / "Silkscreen-Regular.ttf").read_bytes()
    records = dict(tables(original))
    em = struct.unpack_from(">H", records[b"head"], 18)[0]
    assert em == 1000
    raw = [("A", "V", -240), ("V", "A", 125), ("T", "o", -73)]
    pairs = sorted((glyph(records[b"cmap"], a), glyph(records[b"cmap"], b), value)
                   for a, b, value in raw)
    assert len(pairs) == 3
    data = b"".join(struct.pack(">HHh", *pair) for pair in pairs)
    records[b"kern"] = struct.pack(">9H", 0, 1, 0, 14 + len(data), 1, len(pairs), 12, 1, 6) + data
    records[b"name"] = name_table()
    records[b"head"] = records[b"head"][:8] + bytes(4) + records[b"head"][12:]
    size = len(records)
    entry_selector = size.bit_length() - 1
    search_range = 16 * (1 << entry_selector)
    directory = bytearray(struct.pack(">4s4H", original[:4], size, search_range,
                                     entry_selector, 16 * size - search_range))
    payload = bytearray()
    for tag, table in sorted(records.items()):
        offset = 12 + 16 * size + len(payload)
        if tag == b"head":
            patch_at = offset + 8
        directory.extend(struct.pack(">4sIII", tag, checksum(table), offset, len(table)))
        payload.extend(table)
        payload.extend(bytes(-len(table) % 4))
    font = directory + payload
    struct.pack_into(">I", font, patch_at, (0xB1B0AFBA - checksum(font)) & 0xFFFFFFFF)
    return font


def proof_book():
    with zipfile.ZipFile(ROOT / "fast-pair-kerning.epub", "w") as archive:
        for name, data in [
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", """<container version="1.0"
 xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
 <rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""),
            ("book.opf", """<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:identifier id="id">urn:primitive-reader:fast-kerning</dc:identifier>
  <dc:title>Pair spacing specimen</dc:title><dc:language>en-US</dc:language>
 </metadata>
 <manifest><item id="book" href="body.xhtml" media-type="application/xhtml+xml"/></manifest>
 <spine><itemref idref="book"/></spine>
</package>"""),
            ("body.xhtml", """<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
<head><title>Pair spacing specimen</title>
<style>h1, p {font-family: "Harbor Pair Test"; text-align: left; text-indent: 0; hyphens: none;}</style>
</head><body><h1>Pair spacing</h1>
<p>AV Harbor</p><p>VA Signal</p><p>To Lantern</p>
<p>AVAVA at the east pier.</p>
<p>VA and AV between the harbor lights.</p>
</body></html>"""),
        ]:
            archive.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), data)


if __name__ == "__main__":
    (ROOT / "fonts" / "HarborPairTest-Regular.ttf").write_bytes(proof_font())
    proof_book()
