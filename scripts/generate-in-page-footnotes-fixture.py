"""Build the deterministic authored EPUB for separate body and footnote pagination."""
from pathlib import Path
import sys
import zipfile

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
}
manifest = ''.join(f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>'
                   for name in chapters)
spine = ''.join(f'<itemref idref="{name}"/>' for name in chapters)
opf = f'''<package version="3.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-in-page-footnotes</dc:identifier>
<dc:title>Harbor Ledger</dc:title><dc:language>en</dc:language>
<meta property="dcterms:modified">2026-11-24T00:00:00Z</meta></metadata>
<manifest>{manifest}<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest>
<spine>{spine}</spine></package>'''
nav = '<nav epub:type="toc"><ol>' + ''.join(
    f'<li><a href="{name}.xhtml">{name.capitalize()}</a></li>' for name in chapters
) + '</ol></nav>'


def xhtml(title, body):
    return f'''<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en">
<head><title>{title}</title><style type="text/css">
body, p, aside {{ margin: 0; padding: 0; text-indent: 0; text-align: left; orphans: 1; widows: 1; }}
</style></head><body>{body}</body></html>'''


with zipfile.ZipFile(output, "w") as archive:
    def add(name, content, *, stored=False):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
        archive.writestr(entry, content)

    add("mimetype", "application/epub+zip", stored=True)
    add("META-INF/container.xml", container)
    add("content.opf", opf)
    add("nav.xhtml", xhtml("Contents", nav))
    for name, body in chapters.items():
        add(f"{name}.xhtml", xhtml(name, body))
