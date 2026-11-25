#!/usr/bin/env python3
"""Build the offline KOReader WOFF 2 control from Reader's licensed font.

FontTools and its Brotli binding are development-only dependencies. Reader does
not execute this script or require either library at runtime.
"""
import argparse
import hashlib
import io
import json
import runpy
from pathlib import Path

import brotli
import fontTools
from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2FlavorData, WOFF2Reader

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures"
FONT = FIXTURES / "fonts" / "Silkscreen-Regular.ttf"
FONT_SHA256 = "c845473330b94c2079ce9af01c51ac8ba2d99c24f4d14c039843bbb8e642ebd8"
EPUB_NAME = "publisher-woff2.epub"
MANIFEST_NAME = "publisher-woff2.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def outputs():
    if fontTools.__version__ != "4.39.3" or brotli.__version__ != "1.1.0":
        raise RuntimeError("This fixture uses FontTools 4.39.3 and Brotli 1.1.0")
    original = FONT.read_bytes()
    if digest(original) != FONT_SHA256:
        raise RuntimeError("The pinned Silkscreen font has changed")
    encoded = io.BytesIO()
    font = TTFont(io.BytesIO(original), recalcTimestamp=False)
    font.flavor = "woff2"
    font.flavorData = WOFF2FlavorData(transformedTables={"glyf", "loca", "hmtx"})
    font.save(encoded)
    font.close()
    woff2 = encoded.getvalue()
    if not woff2.startswith(b"wOF2"):
        raise RuntimeError("The generated font is not WOFF 2")
    reader = WOFF2Reader(io.BytesIO(woff2))
    transforms = sorted(tag for tag, entry in reader.tables.items() if entry.transformed)
    if transforms != ["glyf", "hmtx", "loca"]:
        raise RuntimeError(f"Expected three transformed tables, found {transforms}")

    base = runpy.run_path(str(ROOT / "scripts" / "generate-fixtures.py"))
    entries = base["publisher_woff"]("woff")
    entries.pop("EPUB/fonts/Silkscreen-Regular.woff")
    entries["EPUB/fonts/Silkscreen-Regular.woff2"] = woff2
    for name in ("EPUB/styles/reader.css", "EPUB/package.opf"):
        text = entries[name]
        assert isinstance(text, str)
        entries[name] = (text.replace("Silkscreen-Regular.woff", "Silkscreen-Regular.woff2")
                             .replace('format("woff")', 'format("woff2")')
                             .replace("font/woff", "font/woff2"))
    book = base["archive"](entries)
    info = {
        "generator": "scripts/generate-woff2-fixture.py",
        "generator_only": {"fonttools": "4.39.3", "brotli": "1.1.0"},
        "source": "fonts/Silkscreen-Regular.ttf",
        "source_sha256": FONT_SHA256,
        "source_license": "OFL-1.1",
        "license_notice": "META-INF/OFL-Silkscreen.txt",
        "transformed_tables": transforms,
        "woff2_sha256": digest(woff2),
        "epub_sha256": digest(book),
    }
    return {EPUB_NAME: book, MANIFEST_NAME: (json.dumps(info, indent=2) + "\n").encode()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for name, content in outputs().items():
        path = FIXTURES / name
        if args.check:
            if not path.is_file() or path.read_bytes() != content:
                parser.error(f"Fixture differs from the generator: {path}")
        else:
            path.write_bytes(content)


if __name__ == "__main__":
    main()
