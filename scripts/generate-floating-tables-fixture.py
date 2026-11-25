#!/usr/bin/env python3
"""Build an EPUB with one horizontal float case per chapter."""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_STORED


def grid(caption, left, right, lower, attributes=""):
    return f"""<table {attributes}>
      <caption><a href="#finish">{caption}</a></caption>
      <tbody><tr><td>{left}</td><td>{right}</td></tr>
      <tr><td colspan="2">{lower}</td></tr></tbody>
      </table>"""


def tail(neighbor, final, clear="both"):
    return (
        f"<p>{neighbor}</p>"
        f'<p id="finish" style="clear:{clear}">{final}</p>'
    )


CHAPTERS = [
    grid(
        "Left caption", "Port cell", "Rail", "Below port",
        'align="LeFt" style="width:200px;caption-side:bottom"',
    ) + tail("Beside port", "After port"),
    grid(
        "Right note", "Quay", "Post", "Below quay",
        'align="right"',
    ) + tail("Beside quay", "After quay"),
    grid(
        "RTL caption", "First cell", "Next cell", "Below RTL",
        'align="left" dir="rtl" style="float:right;width:200px"',
    ) + tail("Beside RTL", "After RTL"),
    grid(
        "Inline caption", "Inline cell", "Inline rail", "Inline lower",
        'align="right" style="float:none;width:200px"',
    ) + tail("Below inline", "After inline"),
    grid(
        "Lead caption", "Lead cell", "Lead rail", "Lead lower",
        'align="left" style="width:200px"',
    ) + grid(
        "Trail caption", "Trail cell", "Trail rail", "Trail lower",
        'style="width:200px;float:right;clear:left"',
    ) + tail("Along leading float", "After right clear", "right"),
    grid(
        "Full caption", "Wide cell", "Wide rail", "Wide lower",
        'style="width:100%;float:left"',
    ) + tail("Below wide float", "After wide float"),
    grid(
        "Centered caption", "Center cell", "Center rail", "Center lower",
        'align="center" style="width:200px"',
    ) + tail("Below centered", "After centered"),
]

STYLE = """body, p { margin: 0; padding: 0; text-indent: 0; }
table { border: 2px solid #111; padding: 4px 7px; border-spacing: 3px; background: #eee; }
caption { border: 1px solid #666; background: #ddd; padding: 2px 3px; text-align: left; }
td { padding: 2px 3px; text-align: left; }"""

CONTAINER = """<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
version="1.0"><rootfiles><rootfile full-path="book.opf"
media-type="application/oebps-package+xml"/></rootfiles></container>"""


def main():
    fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/floating-tables.epub"
    manifest = "".join(
        f'<item id="case{i}" href="case{i}.xhtml" media-type="application/xhtml+xml"/>'
        for i in range(len(CHAPTERS))
    )
    spine = "".join(f'<itemref idref="case{i}"/>' for i in range(len(CHAPTERS)))
    package = f"""<package xmlns="http://www.idpf.org/2007/opf" version="3.0"
    unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">primitiveos-floating-tables</dc:identifier>
    <dc:title>Floating tables</dc:title><dc:language>en</dc:language>
    </metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"""
    with ZipFile(fixture, "w", ZIP_STORED) as archive:
        for name, data in (("mimetype", "application/epub+zip"),
                           ("META-INF/container.xml", CONTAINER), ("book.opf", package)):
            archive.writestr(ZipInfo(name), data.encode())
        for i, content in enumerate(CHAPTERS):
            chapter = f"""<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
            <head><title>Float {i + 1}</title><style>{STYLE}</style></head>
            <body>{content}</body></html>"""
            archive.writestr(ZipInfo(f"case{i}.xhtml"), chapter.encode())


if __name__ == "__main__":
    main()
