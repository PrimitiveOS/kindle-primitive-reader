"""Build the deterministic authored EPUB for separate body and footnote pagination."""
from pathlib import Path
import struct
import sys
import zipfile
import zlib

output = Path(sys.argv[1]) if len(sys.argv) > 1 else (
    Path(__file__).resolve().parents[1] / "tests/fixtures/in-page-footnotes.epub"
)
container = '''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>'''
chapters = {
    "band": '''<p id="start">The harbor ledger opens with a record of the tide.</p>
<p>The east pier was repaired before dawn<a id="ref" epub:type="noteref" href="#note"><sup>1</sup></a>.</p>
<p id="later">A bell by the ferry marked the arrival of the first vessel.</p>
<p>Its keeper left the gate unlocked for the morning crew.</p>
<p>The water rose quietly against the low stone wall.</p>
<p>At noon, the crossing resumed between the two piers.</p>
<p>Three lanterns were stored beside the ferryman's chair.</p>
<p>The last passenger carried a crate with paper labels.</p>
<p>The evening crew found a second crate by the gate.</p>
<p>One note in the margin lists the price of rope.</p>
<p>Another records the height of the spring tide.</p>
<p>A pale stain covers one line of the old paper.</p>
<p>Several entries bear the ferryman's initials.</p>
<p id="end">The account closes before the evening watch.</p>
<aside id="note" epub:type="footnote"><p>The repair date comes from the second harbor ledger.</p>
<p>The original entry names both foremen. <a href="#ref" role="doc-backlink">Return</a></p></aside>''',
    "continuation": '''<p>The shoal appears in the chart<a id="ref" epub:type="noteref" href="#note"><sup>2</sup></a>.</p>
<p id="body-after">The survey party returned by the inland track.</p>
<p id="note" role="doc-footnote">Soundings begin at the western buoy.<br/>'''
        + ''.join(f'<span id="sounding-{index}">Sounding {index:02}: The lead touched gravel at the edge of the bank.</span><br/>'
                  for index in range(1, 18))
        + '<span id="note-end">Soundings end at the upstream marker.</span> <a href="#ref">Return</a></p>',
    "fallback": '''<p>The manuscript contains an interleaved note<a epub:type="noteref" href="#early">1</a>.</p>
<aside id="early" epub:type="footnote"><p>Interleaved note stays in canonical flow.</p></aside>
<p>Narrative resumes below this note.</p>
<aside id="unreferenced" epub:type="footnote"><p>Unreferenced note stays readable.</p></aside>''',
    "boxed-reference": """<p>The northern door opened.</p>
<p style="line-height:1.8">The lower panel beside the lantern was carved with ivy <a id="boxed-ref" href="#boxed-note"
 epub:type="noteref" style="display:inline-block; width:250px; padding-top:68px; padding-bottom:8px; background:#ddd"><span xml:lang="de">2</span></a>.</p>
<p>The ledger kept a separate entry for the south door.</p>
<aside id="boxed-note" epub:type="footnote"><p style="font-size:14px">The original dates from 1807.</p></aside>""",
    "interleaved": """<p id="first">The tide gauge showed an unexpected reading<a id="cross-ref" href="#cross-note" epub:type="noteref"><sup>1</sup></a>.</p>
<p>Coastal observers marked the water on an iron rule.</p>
<p>They kept a dry copy beneath the eastern stair.</p>
<p>The next watch read the upper and lower marks.</p>
<p>A keeper reset the counter before the crossing.</p>
<p>Rain was recorded separately near the channel.</p>
<p>At low water, both crews compared their entries.</p>
<p>The second ledger listed three smaller waves.</p>
<p>Its slip was filed with the weekly tide book.</p>
<p>The final mark was entered before midnight.</p>
<aside id="cross-note" epub:type="footnote"><p>Tide gauge note. <a href="#cross-ref" role="doc-backlink">Return to gauge</a>.</p></aside>
<p id="resume">The gauge register continues with the winter survey.</p>
<p>The final entry describes a loose wooden handle.</p>""",
    "illustrated": """<p id="image-ref">The harbor key explains the dotted line<a id="key-ref" href="#key-note" epub:type="noteref"><sup>2</sup></a>.</p>
<p id="diagram"><a href="#diagram-legend"><img src="chart.png" width="280" height="180" alt="Harbor diagram"/></a></p>
<p id="diagram-legend">The chart legend names the inner and outer routes.</p>
<p>The marker <img src="chart.png" width="48" height="31" alt="Chart marker"/> denotes an old wooden post.</p>
<p>The pilot drew a reference line below the lower pier.</p>
<aside id="key-note" role="doc-footnote"><p>The harbor key note records the chart's revision.</p></aside>""",
    "forced": """<p>The keeper marked a second position<a href="#force-note" id="force-marker" epub:type="noteref">3</a>.</p>
<p style="page-break-before:always" id="force-restart">The harbor record restarts on the next page.</p>
<aside id="force-note" epub:type="footnote"><p>The second position was confirmed later.</p></aside>""",
}
manifest = ''.join(f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>'
                   for name in chapters)
spine = ''.join(f'<itemref idref="{name}"/>' for name in chapters)
opf = f'''<package version="3.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-in-page-footnotes</dc:identifier>
<dc:title>Harbor Ledger</dc:title><dc:language>en</dc:language>
<meta property="dcterms:modified">2026-11-24T00:00:00Z</meta></metadata>
<manifest>{manifest}<item id="chart" href="chart.png" media-type="image/png"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest>
<spine>{spine}</spine></package>'''
nav = '<nav epub:type="toc"><ol>' + ''.join(
    f'<li><a href="{name}.xhtml">{name.capitalize()}</a></li>' for name in chapters
) + '</ol></nav>'


def xhtml(title, body):
    return f'''<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en">
<head><title>{title}</title><style type="text/css">
body, p, aside {{ margin: 0; padding: 0; text-indent: 0; text-align: left; orphans: 1; widows: 1; }}
</style></head><body>{body}</body></html>'''


def chart():
    def chunk(kind, content):
        return struct.pack(">I", len(content)) + kind + content + struct.pack(">I", zlib.crc32(kind + content))

    width, height = 56, 36
    rows = []
    for y in range(height):
        pixels = bytes(35 if x in (1, width - 2) or y in (1, height - 2) or abs(x - 1.5*y) < 1
                       else 175 if (x // 7 + y // 6) % 2 else 235 for x in range(width))
        rows.append(b"\x00" + pixels)
    header = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))


with zipfile.ZipFile(output, "w") as archive:
    def add(name, content, *, stored=False):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
        archive.writestr(entry, content)

    add("mimetype", "application/epub+zip", stored=True)
    add("META-INF/container.xml", container)
    add("content.opf", opf)
    add("chart.png", chart())
    add("nav.xhtml", xhtml("Contents", nav))
    for name, body in chapters.items():
        add(f"{name}.xhtml", xhtml(name, body))
