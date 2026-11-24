"""Build the authored block-rendering comparison EPUB."""

import argparse
from pathlib import Path
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", nargs="?", type=Path)
parser.add_argument("--composition", action="store_true", help="Use the capped box and linked marker")
args = parser.parse_args()
fixture = "render-mode-composition.epub" if args.composition else "render-modes.epub"
epub = args.output or Path(__file__).resolve().parents[1] / "tests/fixtures" / fixture
container = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-render-mode-probe</dc:identifier>
<dc:title>Block rendering specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="main" href="sample.xhtml" media-type="application/xhtml+xml" />
<item id="breaks" href="breaks.xhtml" media-type="application/xhtml+xml" /></manifest>
<spine><itemref idref="main"/><itemref idref="breaks"/></spine>
</package>"""
def chapter(title, css, body):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title><style type="text/css">
body {{ margin:0; padding:0; }}
p, div {{ margin:0; padding:0; text-indent:0; text-align:left; }}
{css}
</style></head><body>{body}</body></html>"""
main = chapter("Block rendering specimen", """
.title { font-size:115%; font-weight:bold; margin-bottom:0.4em }
.collapse-before { margin-bottom:2em }
.collapse-after { margin-top:1em }
.negative-before { margin-top:0.25em; margin-bottom:1.8em }
.negative-after { margin-top:-0.8em }
.underflow-before { margin-bottom:0.2em }
.underflow-after { margin-top:-0.6em }
.center { width:42%; margin-left:auto; margin-right:auto; background-color:#e6e6e6 }
.center-container { width:42%; margin-left:auto; margin-right:auto; background-color:#c8c8c8 }
.explicit { width:260px; height:104px; background-color:#ddd }
.leftward { margin-left:-0.5em }
.float { float:left; width:36%; height:2em; background-color:#ccc }
.clear { clear:both }
.inline-box { display:inline-block; width:42%; background-color:#d2d2d2; }
.css-table { display:table; width:70%; background-color:#eee }
.css-cell { display:table-cell; padding-left:0.5em }
""", """
<p class="title">Block rendering specimen</p>
<p class="collapse-before">Before joined margins</p><p class="collapse-after">After joined margins</p>
<p class="negative-before">Before negative margins</p><p class="negative-after">After negative margins</p>
<p class="underflow-before">Before net negative gap</p><p class="underflow-after">After net negative gap</p>
<p class="center">Centered box marker</p>
<div class="center-container"><p>Centered container marker</p></div>
<p class="explicit">Pixel box marker</p><p class="leftward">After pixel box marker</p>
<div class="float"><p>Floating card marker</p></div>
<p>Neighbor starts here, then the water reaches farther along the shore near the outer channel and the pier.</p>
<p class="clear">Clearance marker</p>
<p>Before inline cluster <span class="inline-box">Inside inline cluster</span> after inline cluster.</p>
<div class="css-table"><div class="css-cell">First table cell</div><div class="css-cell">Second table cell</div></div>
<p>End of specimen</p>""")
breaks = chapter("Empty break specimen", """
.empty-break { page-break-before:always; page-break-after:always; margin:0.5em 0; padding:0.25em; border-top:2px solid #888; }
.nonempty-break { page-break-before:always; }
""", """
<p>Before empty breaks</p>
<div class="empty-break"></div><div class="empty-break"></div>
<p>After empty breaks</p>
<p class="nonempty-break">After nonempty break</p>
<p>End of breaks</p>""")
if args.composition:
    main = main.replace(".explicit { width:260px;", ".explicit { width:400px; max-width:260px;")
    main = main.replace(
        '<p class="leftward">After pixel box marker</p>',
        '<p class="leftward"><a href="#source-target">After pixel box marker</a></p>',
    )
    main = main.replace("<p>End of specimen</p>", '<p id="source-target">End of specimen</p>')
with zipfile.ZipFile(epub, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf),
        ("sample.xhtml", main),
        ("breaks.xhtml", breaks),
    ]:
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)
