"""Build the authored EPUB for conflicting opening and closing break rules."""

from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-opening-breaks.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture

pairs={
    'direct-before':('<div class="keep"><p>Direct before previous</p><p class="force-before">Direct before next</p></div>', 'Direct before previous', 'Direct before next'),
    'direct-after':('<div class="keep"><p class="force-after">Direct after previous</p><p>Direct after next</p></div>', 'Direct after previous', 'Direct after next'),
    'nested-before':('<div class="keep"><p>Nested before previous</p><div><p class="force-before">Nested before next</p></div></div>', 'Nested before previous', 'Nested before next'),
    'nested-after':('<div class="keep"><div><p class="force-after">Nested after previous</p></div><p>Nested after next</p></div>', 'Nested after previous', 'Nested after next'),
    'adjacent-before':('<p class="keep-after">Adjacent before previous</p><p class="force-before">Adjacent before next</p>', 'Adjacent before previous', 'Adjacent before next'),
    'adjacent-after':('<p class="force-after">Adjacent after previous</p><p class="keep-before">Adjacent after next</p>', 'Adjacent after previous', 'Adjacent after next'),
    'plain-after':('<p class="force-after">Standalone after previous</p><p>Standalone after next</p>', 'Standalone after previous', 'Standalone after next'),
    'plain-before':('<p>Standalone before previous</p><p class="force-before">Standalone before next</p>', 'Standalone before previous', 'Standalone before next'),
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
.keep-before { page-break-before:avoid; }
.keep-after { page-break-after:avoid; }
.box { background:#ddd; }
</style></head><body><p>Carry-over text</p>''' + body + '<p>Following body text</p></body></html>'
names=''.join(f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>' for name in pairs)
order=''.join(f'<itemref idref="{name}"/>' for name in pairs)
opf=f'''<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">publisher-opening-breaks</dc:identifier>
<dc:title>Publisher opening break specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest>{names}</manifest><spine>{order}</spine></package>'''

def write(book, name, contents):
    entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
    book.writestr(entry, contents)

with zipfile.ZipFile(output, "w") as book:
    write(book, "mimetype", "application/epub+zip")
    write(book, "META-INF/container.xml", container)
    write(book, "content.opf", opf)
    for name, (body, _, _) in pairs.items():
        write(book, name + ".xhtml", chapter(body))
