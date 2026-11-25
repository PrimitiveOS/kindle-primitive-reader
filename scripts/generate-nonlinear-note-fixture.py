#!/usr/bin/env python3
"""Build the original, public test book used by both EPUB flow replays."""
from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo


def xhtml(title, body):
    return (
        '<html xmlns="http://www.w3.org/1999/xhtml" '
        'xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en">'
        f"<head><title>{title}</title></head><body>{body}</body></html>"
    )


members = {
    "mimetype": "application/epub+zip",
    "META-INF/container.xml": (
        '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">'
        '<rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/>'
        "</rootfiles></container>"
    ),
    "book.opf": (
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="edition">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        '<dc:identifier id="edition">urn:primitiveos:lantern-survey</dc:identifier>'
        "<dc:title>The Lantern Survey</dc:title><dc:language>en</dc:language>"
        '<meta property="dcterms:modified">2026-11-25T00:00:00Z</meta></metadata>'
        '<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
        '<item id="survey" href="survey.xhtml" media-type="application/xhtml+xml"/>'
        '<item id="route" href="route.xhtml" media-type="application/xhtml+xml"/>'
        '<item id="return" href="return.xhtml" media-type="application/xhtml+xml"/>'
        '</manifest><spine><itemref idref="survey"/><itemref idref="route"/>'
        '<itemref idref="return"/></spine></package>'
    ),
    "nav.xhtml": xhtml(
        "Contents",
        '<nav epub:type="toc"><ol><li><a href="survey.xhtml#start">The first light</a></li>'
        '<li><a href="route.xhtml#route">The outer route</a></li>'
        '<li><a href="return.xhtml#start-return">The return</a></li></ol></nav>',
    ),
    "survey.xhtml": xhtml(
        "The first light",
        '<h1 id="start">The first light</h1>'
        '<p id="reference-one">The harbor opened below the ridge. I copied the first '
        'signal <a epub:type="noteref" href="#note-a">[1]</a> and the lantern interval '
        '<a epub:type="noteref" href="#note-b">[2]</a> into the field book.</p>'
        '<aside epub:type="footnote" id="note-a"><p>Red lantern schedule. The eastern '
        'light is lit at dusk. <a href="#reference-one">Return to the survey</a>.</p></aside>'
        '<aside epub:type="footnote" id="note-b"><p>Interval of four beats. Listen from '
        'the sheltered landing. <a href="#reference-one">Back to the first entry</a>.</p></aside>'
        '<p id="middle">The bridge remained clear. I marked the channel '
        '<a epub:type="noteref" href="#note-c">[3]</a> beside the lower steps.</p>'
        '<aside epub:type="footnote" id="note-c"><p>Lower channel notation. At low tide '
        'the marker leans west. <a href="#middle">Return to the bridge</a>.</p></aside>'
        '<p id="finish">We closed the field book and returned to the boats.</p>',
    ),
    "route.xhtml": xhtml(
        "The outer route",
        '<h1 id="route">The outer route</h1><p>White water broke against the cliffs.</p>',
    ),
    "return.xhtml": xhtml(
        "The return",
        '<h1 id="start-return">The return</h1>'
        '<p id="reference-return">At dawn we checked the north light '
        '<a epub:type="noteref" href="#note-d">[4]</a> from the headland.</p>'
        '<aside epub:type="footnote" id="note-d"><p>Morning reading. The red signal had '
        'been put out. <a href="#reference-return">Return to the headland</a>.</p></aside>'
        '<p id="last">The survey ended at the far steps.</p>',
    ),
}

output = Path(__file__).resolve().parents[1] / "tests/fixtures/nonlinear-note-flows.epub"
with ZipFile(output, "w", compression=ZIP_STORED) as archive:
    for path, content in members.items():
        archive.writestr(ZipInfo(path, date_time=(2000, 1, 1, 0, 0, 0)), content)
