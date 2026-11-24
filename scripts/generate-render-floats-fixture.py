"""Build the authored EPUB used for float and clearance measurements."""
from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-floats.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = '''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>'''
sheet = '''body,p,div { margin:0; padding:0; text-indent:0; text-align:left; }
.left { float:left; width:36%; height:2em; background-color:#ccc }
.right { float:right; width:36%; height:2em; background-color:#ddd }
.low { height:4em } .tall { height:5em }
.clear-left { clear:left } .clear-right { clear:right } .clear-both { clear:both }
.broken { width:99999px } .pinched { width:80px }'''
pages = {
    "left": '''<div class="left"><p>Left card</p></div>
<p><a href="#left-end">Left neighbor</a> falls beside this card.</p>
<p class="clear-left" id="left-end">Left clearance</p>
<p>Left ordinary paragraph.</p>''',
    "right": '''<div class="right"><p>Right card</p></div>
<p><a href="#right-end">Right neighbor</a> falls beside this card.</p>
<p class="clear-right" id="right-end">Right clearance</p>
<p>Right ordinary paragraph.</p>''',
    "opposed": '''<div class="left low"><p>Opposed left</p></div>
<div class="right tall"><p>Opposed right</p></div>
<p>Opposing neighbor</p>
<p class="clear-left">Left cleared only</p>
<p class="clear-both">Both sides cleared</p>''',
    "wrap": '''<div class="left"><p>Wrap card</p></div>
<p>Wrap begins here while the tall reed by the channel casts a shadow all the way across the old plank bridge. This paragraph goes on long enough to read alongside the float for several lines. When the paragraph reaches the bottom of the float, it should return to the full text measure on the shore. Wrap ending marker.</p>
<p>Wrap next paragraph.</p>''',
    "fallback": '''<div class="left broken"><p>Malformed width card</p></div>
<div class="right"><p>Nested card first</p><p>Nested card second</p></div>
<p class="clear-both">Fallback ordinary paragraph.</p>''',
    "rollback": '<div class="left pinched"><p>' + 'Reed ' * 60
        + '</p></div><p>Readable after geometry rollback.</p>',
}
manifest = "".join(
    f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>'
    for name in pages
)
spine = "".join(f'<itemref idref="{name}"/>' for name in pages)
opf = f'''<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-float-probe</dc:identifier>
<dc:title>Float layout specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest>{manifest}</manifest><spine>{spine}</spine></package>'''
with zipfile.ZipFile(output, "w") as archive:
    def add(name, content, *, stored=False):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
        archive.writestr(entry, content)

    add("mimetype", "application/epub+zip", stored=True)
    add("META-INF/container.xml", container)
    add("content.opf", opf)
    for name, body in pages.items():
        add(f"{name}.xhtml", f'''<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{name}</title><style type="text/css">{sheet}</style></head>
<body>{body}</body></html>''')
