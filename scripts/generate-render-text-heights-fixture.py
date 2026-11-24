"""Build the authored ordinary-paragraph height comparison EPUB."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-text-heights.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-render-text-heights</dc:identifier>
<dc:title>Text box height specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="main" href="sample.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="main"/></spine>
</package>"""
xhtml = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Text box height specimen</title><style type="text/css">
body { margin:0; padding:0; }
p { margin:0; padding:0; text-indent:0; text-align:left; }
.control { background:#ebebeb; }
.tall { height:164px; background:#e0e0e0; }
.short { height:24px; background:#d6d6d6; }
.percent { height:45%; background:#ccc; }
.wide { width:420px; height:153px; background:#c2c2c2; }
.fill { height:600px; background:#b8b8b8; }
</style></head><body>
<p class="control">Natural height</p><p>Control follows</p>
<p class="tall">Tall height</p><p>Tall follows</p>
<p class="short">Short height<br/>Short continuation</p><p>Short follows</p>
<p class="percent">Percent height</p><p>Percent follows</p>
<p class="wide">Web wide and tall</p><p>Width follows</p>
<p class="fill">Page fill height</p><p>Last line</p>
</body></html>"""
with zipfile.ZipFile(output, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf),
        ("sample.xhtml", xhtml),
    ]:
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)
