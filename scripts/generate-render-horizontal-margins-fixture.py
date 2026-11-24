"""Build an EPUB with authored physical margin cases."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-horizontal-margins.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-horizontal-margins</dc:identifier>
<dc:title>Horizontal margin specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="sample" href="sample.xhtml" media-type="application/xhtml+xml" /></manifest>
<spine><itemref idref="sample"/></spine></package>"""
chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Horizontal margin specimen</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.pixels { margin-left:40px; margin-right:24px; background-color:#ddd }
.relative { margin-left:1.4em; margin-right:2%; background-color:#ccc }
.minimum { margin-left:0.01em; margin-right:auto; background-color:#bbb }
</style></head><body>
<p>Opening ordinary text</p>
<p class="pixels">Physical sides marker. The midmorning soundings run from the pier to the flats and back to the mouth of the channel.</p>
<p class="relative">Font and percent marker. The east bank shows the marks left by a rising tide and the boat turns into the current.</p>
<p class="minimum">Minimum inset marker</p>
<p>Following ordinary text</p>
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
