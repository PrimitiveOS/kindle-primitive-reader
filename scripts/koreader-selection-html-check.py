#!/usr/bin/env python3
"""Run the selection HTML callback in a pinned KOReader macOS runtime."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

CONTAINER = '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'
PACKAGE = (
    '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">'
    '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
    '<dc:identifier id="id">primitiveos-selection-html</dc:identifier>'
    '<dc:title>Harbor notebook</dc:title><dc:language>en</dc:language></metadata>'
    '<manifest><item id="book" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest>'
    '<spine><itemref idref="book"/></spine></package>'
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True, help="runtime/koreader")
    parser.add_argument("--staging", type=Path, required=True, help="build/staging")
    options = parser.parse_args()
    runtime = options.runtime.resolve()
    scripts = Path(__file__).resolve().parent
    fixture = scripts.parent / "tests/fixtures/selection-html.xhtml"

    with tempfile.TemporaryDirectory(prefix="primos-stock-selection-") as scratch:
        root = Path(scratch)
        home = root / "home"
        patches = home / "patches"
        patches.mkdir(parents=True)
        (patches / "2-qa-selection-html.lua").write_bytes(
            (scripts / "koreader-selection-html-check.lua").read_bytes()
        )
        (home / "settings.reader.lua").write_text(
            'return {language="en", external_dict_lookup=false, '
            'emulator_fake_wifi_connected=false, color_rendering=false, use_xtext=true}\n'
        )
        book = root / "Selection HTML.epub"
        with zipfile.ZipFile(book, "w") as epub:
            epub.writestr("mimetype", "application/epub+zip")
            epub.writestr("META-INF/container.xml", CONTAINER)
            epub.writestr("OPS/book.opf", PACKAGE)
            epub.write(fixture, "OPS/chapter.xhtml")

        environment = dict(
            os.environ,
            KO_HOME=str(home),
            KO_DYLD_PREFIX=str(options.staging.resolve()),
            EMULATE_READER_W="632",
            EMULATE_READER_H="840",
            EMULATE_READER_DPI="300",
            EMULATE_BW_SCREEN="1",
            EMULATE_BB_TYPE="BB8",
        )
        subprocess.run(
            [str(runtime / "luajit"), "reader.lua", str(book)],
            cwd=runtime,
            env=environment,
            timeout=25,
            check=True,
        )


if __name__ == "__main__":
    main()
