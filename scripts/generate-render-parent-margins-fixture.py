"""Build the authored parent-child margin collapse comparison EPUB."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-parent-margins.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-render-parent-margins</dc:identifier>
<dc:title>Parent margin specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="main" href="sample.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="main"/></spine>
</package>"""
document = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Parent margin specimen</title><style type="text/css">
body,div,p { margin:0;padding:0;text-indent:0;text-align:left; }
</style></head><body>
<p>Natural first</p><p>Natural second</p>
<p style="margin-bottom:28px;">Before joined</p>
<div style="margin:42px 0 32px;">
<p style="margin:56px 0 18px;background:#ddd;">Joined first</p>
<p style="margin:12px 0 44px;background:#ddd;">Joined last</p></div>
<p style="margin-top:38px;">After joined</p>
<p style="margin-bottom:10px;">Before signed</p>
<div style="margin:8px 0 15px;">
<p style="margin:29px 0 0;background:#ccc;">Signed first</p>
<p style="margin-bottom:-28px;background:#ccc;">Signed last</p></div>
<p style="margin-top:-17px;">After signed</p>
<p style="margin-bottom:15px;">Before barrier</p>
<div style="margin:26px 0 21px;padding:8px 0 9px;">
<p style="margin:37px 0 32px;background:#bbb;">Barrier first</p>
<p style="margin:16px 0 43px;background:#bbb;">Barrier last</p></div>
<p style="margin-top:20px;">After barrier</p>
<p style="margin-bottom:12px;">Before text</p>
<div style="margin:20px 0;">Intervening parent text<p style="margin:34px 0;">Text child</p></div>
<p style="margin-bottom:15px;">Before border</p>
<div style="margin:26px 0 21px;border-top:8px solid #888;border-bottom:9px solid #888;">
<p style="margin:37px 0 43px;">Border child</p></div>
<p style="margin-top:20px;">After border</p>
<p style="margin-bottom:33px;">Before nested</p>
<div style="margin:14px 0 51px;"><div>
<p style="margin:38px 0 -21px;background:#c8c8c8;">Nested child</p></div></div>
<p style="margin-top:22px;">After nested</p>
</body></html>"""
with zipfile.ZipFile(output, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf),
        ("sample.xhtml", document),
    ]:
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)
