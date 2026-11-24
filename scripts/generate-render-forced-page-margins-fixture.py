"""Build the authored comparison of margins at forced publisher page breaks."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-forced-page-margins.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture

pairs={
    'retained-top':('<p style="margin-bottom:94px">Margin before previous</p><p class="force-before box" style="margin-top:68px">Margin before next</p>', 'Margin before previous', 'Margin before next'),
    'after-with-top':('<p class="force-after" style="margin-bottom:94px">Margin after previous</p><p class="box" style="margin-top:68px">Margin after next</p>', 'Margin after previous', 'Margin after next'),
    'plain-margin':('<p style="margin-bottom:94px">Ordinary margin previous</p><p class="box" style="margin-top:68px">Ordinary margin next</p>', 'Ordinary margin previous', 'Ordinary margin next'),
    'zero-top':('<p style="margin-bottom:94px">Zero margin previous</p><p class="force-before box">Zero margin next</p>', 'Zero margin previous', 'Zero margin next'),
    'kept-after':('<div class="keep"><p class="force-after" style="margin-bottom:94px">Kept after previous</p><p class="box" style="margin-top:68px">Kept after next</p></div>', 'Kept after previous', 'Kept after next'),
}
container='''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>'''
def chapter(body):
    return '''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Explicit break specimen</title>
<style type="text/css">
body, div, p { margin:0; padding:0; text-indent:0; text-align:left; }
.keep { page-break-inside:avoid; }
.force-before { page-break-before:always; }
.force-after { page-break-after:always; }
.box { background:#ddd; }
</style></head><body><p>Carry-over text</p>''' + body + '<p>Following body text</p></body></html>'
names=''.join(f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>' for name in pairs)
order=''.join(f'<itemref idref="{name}"/>' for name in pairs)
opf=f'''<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">forced-page-margins</dc:identifier>
<dc:title>Forced page margin specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest>{names}</manifest><spine>{order}</spine></package>'''

def write(archive, name, contents):
    entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
    archive.writestr(entry, contents)

with zipfile.ZipFile(output, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("content.opf", opf),
        ("META-INF/container.xml", container),
    ]:
        write(book, name, contents)
    for name, (body, _, _) in pairs.items():
        write(book, name + ".xhtml", chapter(body))
