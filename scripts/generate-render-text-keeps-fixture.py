"""Build the authored comparison of fitting and oversized publisher keep groups."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-text-keeps.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture

container='''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>'''
style='''body, div, p { margin:0; padding:0; text-indent:0; text-align:left; }
.keep { page-break-inside:avoid }
.shaded { background:#ddd }'''
spine={
"fits": '''<p>Opening line</p><p>Regular line two</p><p>Regular line three</p>
<p>Regular line four</p><p>Regular line five</p><p>Regular line six</p><p>Before short group</p>
<div class="keep"><p class="shaded">Short opening</p><p>Short middle</p><p>Short ending</p></div>
<p>After short group</p>''',
"oversized": '''<p>Prelude one</p><p>Prelude two</p><p>Prelude three</p><p>Before oversized group</p>
<div class="keep"><p class="shaded">Long opening</p>
<p>Long line two</p><p>Long line three</p><p>Long line four</p><p>Long line five</p><p>Long middle</p>
<p>Long line seven</p><p>Long line eight</p><p>Long line nine</p><p>Long line ten</p><p>Long line eleven</p>
<p>Long ending</p></div><p>After oversized group</p>''',
}
manifest=''.join(f'<item id="{k}" href="{k}.xhtml" media-type="application/xhtml+xml"/>' for k in spine)
order=''.join(f'<itemref idref="{k}"/>' for k in spine)
opf=f'''<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-break-group</dc:identifier>
<dc:title>Page break group specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest>{manifest}</manifest><spine>{order}</spine></package>'''

with zipfile.ZipFile(output, "w") as book:
    def write(name, contents):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)

    write("mimetype", "application/epub+zip")
    write("META-INF/container.xml", container)
    write("content.opf", opf)
    for section, body in spine.items():
        xhtml = f"""<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Break group</title><style type="text/css">{style}</style></head><body>{body}</body></html>"""
        write(f"{section}.xhtml", xhtml)
