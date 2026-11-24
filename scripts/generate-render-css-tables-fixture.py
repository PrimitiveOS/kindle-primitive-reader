#!/usr/bin/env python3
"""Generate the authored CSS table repair EPUB."""
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
EPUB = ROOT / "tests/fixtures/render-css-tables.epub"
container = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
package = """<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">primos-authored-css-tables</dc:identifier>
<dc:title>Harbor watch tables</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="harbor" href="harbor.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="harbor"/></spine>
</package>"""
chapter = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Harbor watch tables</title>
<style type="text/css">
body, p, div { margin:0; padding:0; text-indent:0; text-align:left; }
.grid {display:table; background-color:#ddd; width:70%}
.row {display:table-row}
.group {display:table-row-group}
.cell {display:table-cell; padding-left:10px; width:50%; background-color:#bbb}
.natural {width:auto}
.hide {display:none}
</style></head><body>
<p>Before harbor table.</p>
<div class="grid"><div class="cell" id="pier">Pier</div><div class="cell" id="buoy"><a href="#after">Buoy</a></div>
<div class="group"><div class="row"><div class="cell" id="north">North</div><div class="cell" id="south">South</div></div></div></div>
<p>Between harbor tables.</p>
<div><div class="cell natural" id="ebb">Ebb</div><div class="cell natural" id="flood">Flood</div><p>Visible separator.</p>
<div class="row"><div class="cell" id="light">Light</div><div class="cell" id="dark">Dark</div></div>
<div class="row"><div class="cell" id="sand">Sand</div><div class="cell" id="rock">Rock</div></div></div>
<div class="grid"><div class="cell" id="watch">Watch</div><div class="hide">Hidden</div><div class="cell" id="dawn">Dawn</div>
<div class="row"><div class="cell" id="rope">Rope</div><div class="cell" id="mast">Mast</div></div></div>
<p id="after">After harbor table.</p>
</body></html>"""
with zipfile.ZipFile(EPUB, "w") as book:
    for name, content in (
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", package),
        ("harbor.xhtml", chapter),
    ):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, content)
