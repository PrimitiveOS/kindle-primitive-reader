"""Build an EPUB with authored fixed and constrained text box widths."""

from pathlib import Path
import sys
import zipfile

signed = "--signed" in sys.argv[1:]
name = "render-signed-widths.epub" if signed else "render-width-margins.epub"
fixture = Path(__file__).resolve().parents[1] / "tests/fixtures" / name
paths = [value for value in sys.argv[1:] if value != "--signed"]
output = Path(paths[0]) if paths else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-width-margins</dc:identifier>
<dc:title>Width and Margin Soundings</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="sample" href="sample.xhtml" media-type="application/xhtml+xml" /></manifest>
<spine><itemref idref="sample"/></spine></package>"""
chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Width and Margin Soundings</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.physical { width:320px; margin-left:40px; margin-right:24px; background:#ddd }
.trailing { width:40%; margin-left:auto; margin-right:56px; background:#ccc }
.maximum { width:520px; max-width:380px; min-width:240px; margin:0 auto; background:#bbb }
.minimum { width:300px; max-width:210px; min-width:36%; margin:0 auto; background:#aaa }
.cap { max-width:420px; margin:0 auto; background:#999 }
.overconstraint { width:1000px; margin-left:80px; margin-right:120px; background:#888 }
.direction { width:320px; margin-left:40px; margin-right:24px; text-align:right; background:#777 }
.free { max-width:1200px; margin-left:52px; margin-right:12px; background:#666 }
</style></head><body>
<p>Opening field markers</p>
<p class="physical"><a href="#west">Fixed marker</a> charts the landing at the west pier and the path up the steep bank.</p>
<p class="trailing">Trailing marker</p>
<p class="maximum">Maximum marker</p>
<p class="minimum">Minimum marker</p>
<p class="cap">Auto cap marker</p>
<p class="overconstraint">Overflow marker</p>
<p class="direction" dir="rtl">Direction marker</p>
<p class="free">Natural marker</p>
<p id="west">West bank directions</p>
</body></html>"""

if signed:
    opf = opf.replace("reader-width-margins", "reader-signed-widths")
    chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Signed width soundings</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.left { width:400px; margin-left:-24px; margin-right:12px; background:#ddd }
.right { width:480px; margin-left:auto; margin-right:-24px; text-align:right; background:#ccc }
.wide { width:1300px; margin-left:auto; margin-right:-24px; text-align:right; background:#bbb }
.bleed { width:1240px; margin-left:-74px; margin-right:40px; background:#aaa }
.direction { width:1100px; margin-left:40px; margin-right:-78px; text-align:right; background:#999 }
.cap { max-width:420px; margin-left:-32px; margin-right:auto; background:#888 }
.minimum { min-width:112%; max-width:380px; margin-left:-34px; margin-right:8px; text-align:right; background:#777 }
.relative { width:104%; margin-left:auto; margin-right:-84px; text-align:right; background:#666 }
</style></head><body>
<p>Reading the channel</p>
<p class="left"><a href="#channel">Left beacon</a> marks the entrance to the west channel.</p>
<p class="right">Look for the <a href="#channel">Right beacon</a></p>
<p class="wide">The rope trails toward the <a href="#channel">Reef line</a></p>
<p class="bleed"><a href="#channel">Low headland</a> stays in view beside the bay.</p>
<p class="direction" dir="rtl">Turning basin</p>
<p class="cap">One small lookout</p>
<p class="minimum">The staff rises above the <a href="#channel">Saltbush</a></p>
<p class="relative">Look out beyond the <a href="#channel">Ebb gate</a></p>
<p id="channel">West channel directions</p>
</body></html>"""

with zipfile.ZipFile(output, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf),
        ("sample.xhtml", chapter),
    ]:
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)
