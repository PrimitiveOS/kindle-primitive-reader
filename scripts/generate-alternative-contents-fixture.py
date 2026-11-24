#!/usr/bin/env python3
"""Build the small authored EPUB used by Reader and the KOReader comparison."""
from pathlib import Path
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/alternative-contents.epub"
chapters = [
    '<p>A quiet preface without a heading.</p>',
    '<h1>Harbor Survey</h1><p>The departure ledger includes a morning tide.</p>'
    '<h3>East soft\u00adhyphen quay</h3><p>An east quay field note.</p>'
    '<h2>West quay</h2><p>A west quay field note.</p>',
    '<p>An appendix without headings.</p>',
]


def make(path, headings):
    with zipfile.ZipFile(path, 'w') as book:
        book.writestr('mimetype', 'application/epub+zip')
        book.writestr('META-INF/container.xml', '''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>''')
        items = ''.join(f'<item id="part{i}" href="part{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(3))
        spine = ''.join(f'<itemref idref="part{i}"/>' for i in range(3))
        book.writestr('content.opf', f'''<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">primitive-reader-alternative-contents</dc:identifier><dc:title>Harbor Survey</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata>
<manifest><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>{items}</manifest><spine>{spine}</spine></package>''')
        book.writestr('nav.xhtml', '''<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="part0.xhtml">Publisher preface</a></li><li><a href="part1.xhtml">Publisher voyage</a></li><li><a href="part2.xhtml">Publisher appendix</a></li></ol></nav></body></html>''')
        for i, chapter in enumerate(chapters):
            if not headings and i == 1:
                chapter = '<p>An unheaded voyage log.</p>'
            book.writestr(f'part{i}.xhtml', f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Part {i + 1}</title></head><body>{chapter}</body></html>')


if __name__ == '__main__':
    make(fixture, True)
    make(fixture.with_name('alternative-contents-fragments.epub'), False)
