#!/usr/bin/env python3
"""Write the deterministic EPUB used by the hyphenation options comparisons."""
import sys
import zipfile

from pathlib import Path

chapter = Path(__file__).resolve().parents[1] / "tests/fixtures/hyphenation-options.xhtml"
with zipfile.ZipFile(sys.argv[1], "w") as book:
    for name, content in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml",
         '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">'
         '<rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/>'
         "</rootfiles></container>"),
        ("book.opf",
         '<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">'
         '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
         '<dc:identifier id="id">urn:primitiveos:reader:hyphenation-options</dc:identifier>'
         "<dc:title>Hyphenation specimen</dc:title><dc:language>en-US</dc:language></metadata>"
         '<manifest><item id="body" href="body.xhtml" media-type="application/xhtml+xml"/></manifest>'
         '<spine><itemref idref="body"/></spine></package>'),
        ("body.xhtml", chapter.read_bytes()),
    ]:
        entry = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
        book.writestr(entry, content)
