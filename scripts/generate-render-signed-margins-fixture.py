"""Build the authored left and right negative inset EPUB."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-signed-margins.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-signed-margins</dc:identifier>
<dc:title>Signed Margin Soundings</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="sample" href="sample.xhtml" media-type="application/xhtml+xml" /></manifest>
<spine><itemref idref="sample"/></spine></package>"""
chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Signed Margin Soundings</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.leftward { margin-left:-0.5em; margin-right:12px; background:#ddd }
.rightward { margin-left:12px; margin-right:-0.5em; background:#ccc; text-align:right }
.rightbound { text-align:right }
a { text-decoration:underline }
</style></head><body>
<p>Ashore, the first crew waited on the turn of the tide.</p>
<p class="leftward"><a href="#west">Ashore</a> at low tide, leave the painted post to the west.</p>
<p class="rightbound">The steersman writes the last word as wind</p>
<p class="rightward">Pass the beacon before the southwest <a href="#east">wind</a></p>
<p id="west">West landing instructions</p>
<p id="east">East landing instructions</p>
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
