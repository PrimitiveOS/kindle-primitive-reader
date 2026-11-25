#!/usr/bin/env python3
"""Generate Reader's RTF OEM lookup tables from the retained Unicode files."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
chunks = [
    "// Generated from Unicode's CP437.TXT and CP850.TXT mapping data.\n",
    "// Source files and Unicode copyright/permission notice are distributed\n",
    "// in reader/licenses; see docs/licenses/RTF-OEM.txt.\n\n",
]
for page in (437, 850):
    source = ROOT / f"assets/rtf-codepages/CP{page}.TXT"
    mapping = {}
    for line in source.read_text(encoding="ascii").removesuffix("\x1a").splitlines():
        row = line.split("#", 1)[0].split()
        if not row:
            continue
        if len(row) != 2:
            raise ValueError(f"Unexpected mapping in {source}")
        byte, scalar = (int(value, 16) for value in row)
        if byte in mapping or byte > 255 or scalar > 0x10FFFF or 0xD800 <= scalar <= 0xDFFF:
            raise ValueError(f"Invalid mapping in {source}")
        mapping[byte] = scalar
    if len(mapping) != 256 or any(mapping.get(n) != n for n in range(128)):
        raise ValueError(f"Unexpected ASCII or incomplete mapping in {source}")
    chunks.append(f"pub(super) static CP{page}: [char; 128] = [\n")
    for offset in range(128, 256, 8):
        row = ", ".join(f"'\\u{{{mapping[byte]:x}}}'" for byte in range(offset, offset + 8))
        chunks.append(f"    {row},\n")
    chunks.append("];\n\n")
target = ROOT / "src/rtf/oem_tables.rs"
target.parent.mkdir(parents=True, exist_ok=True)
target.write_text("".join(chunks), encoding="utf-8")
