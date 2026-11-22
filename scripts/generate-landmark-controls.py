#!/usr/bin/env python3
"""Write five authored EPUB navigation controls for Reader and KOReader."""

import argparse
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile, ZipInfo

BASE = Path(__file__).resolve().parents[1] / "tests/fixtures/landmark-survey.epub"
PRIMARY = '<nav epub:type="toc"><ol><li><a href="first.xhtml#start">Primary entry</a></li></ol></nav>'
NCX_ITEM = '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
NCX = b'<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="ch1" playOrder="1"><navLabel><text>NCX entry</text></navLabel><content src="first.xhtml#start"/></navPoint></navMap></ncx>'


def write_control(output, name, baseline):
    members = dict(baseline)
    nav = members["nav.xhtml"].decode()
    opf = members["book.opf"].decode().replace("reader-landmark-landmarks", "reader-landmark-" + name)
    start = nav.index('<nav epub:type="landmarks">')
    pages = nav.index('<nav epub:type="page-list">')
    if name == "primary":
        nav = nav[:start] + PRIMARY + nav[start:]
    elif name == "pages":
        nav = nav[:start] + nav[pages:]
    elif name == "landmarks-before-ncx":
        opf = opf.replace("</manifest>", NCX_ITEM + "</manifest>").replace("<spine>", '<spine toc="ncx">')
        members["toc.ncx"] = NCX
    elif name == "epub2-guide":
        opf = opf.replace('version="3.0"', 'version="2.0"').replace(
            '<item id="menu" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>', ""
        )
        nav = nav[:start] + nav[nav.index("</body>"):]
    members.update({"book.opf": opf.encode(), "nav.xhtml": nav.encode()})
    with ZipFile(output / (name + ".epub"), "w") as book:
        for member, data in members.items():
            entry = ZipInfo(member, (2024, 1, 1, 0, 0, 0))
            entry.compress_type = ZIP_STORED if member == "mimetype" else ZIP_DEFLATED
            book.writestr(entry, data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="directory for the authored EPUBs")
    output = parser.parse_args().output
    output.mkdir(parents=True, exist_ok=True)
    with ZipFile(BASE) as book:
        members = {name: book.read(name) for name in book.namelist()}
    for name in ("primary", "landmarks", "landmarks-before-ncx", "pages", "epub2-guide"):
        write_control(output, name, members)


if __name__ == "__main__":
    main()
