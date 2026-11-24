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
.broken { width:99999px } .pinched { width:80px }
.long-edge { width:200px; height:160px; background:#ccc }
.inner-edge { width:300px; height:80px; background:#bbb }
.opposite { width:240px; height:240px; background:#ddd }
.outer-wide { width:680px; height:96px; background:#ccc }
.dropped-wide { width:400px; height:160px; background:#bbb }
.small-right { width:160px; height:120px; background:#aaa }
.clearing { width:260px; height:80px; background:#bbb }
.right-inner { width:300px; height:80px; background:#aaa }
.dense { width:150px; height:160px; background:#ccc }
.dense.short { height:80px; background:#bbb }'''
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
    "stacked-left": '''<div class="left long-edge"><p>Long edge</p></div>
<div class="left inner-edge"><p>Inner edge</p></div>
<div class="right opposite"><p>Opposite bank</p></div>
<p><a href="#shelf-end">Upper shelf</a></p>
<p>Middle shelf</p>
<p>Lower shelf</p>
<p class="clear-left" id="shelf-end">Open bank</p>
<p class="clear-both">Clear bank</p>''',
    "stacked-right": '''<div class="right opposite"><p>Outer quay</p></div>
<div class="right right-inner"><p>Inner quay</p></div>
<p>Below the crane</p>
<p class="clear-right">Clear quay</p>''',
    "dropped-floats": '''<div class="left outer-wide"><p>Outer card</p></div>
<div class="right opposite"><p>Right wall</p></div>
<div class="left dropped-wide"><p>Lower card</p></div>
<div class="right small-right"><p>Small card</p></div>
<p>Water edge</p>
<p class="clear-both">Clear water</p>''',
    "clear-on-float": '''<div class="left long-edge"><p>First beam</p></div>
<div class="left clearing clear-left"><p>Next beam</p></div>
<p>Along beam</p>
<p class="clear-both">After beams</p>''',
    "stacked-dense": '''<div class="left dense"><p>A</p></div>
<div class="left dense"><p>B</p></div>
<div class="left dense"><p>C</p></div>
<div class="left dense"><p>D</p></div>
<div class="left dense"><p>E</p></div>
<div class="left dense short"><p>Far step</p></div>
<p>High ledge</p>
<p>Mid ledge</p>
<p>Low ledge</p>
<p class="clear-both">Clear ledge</p>''',
    "float-budget": '<p style="page-break-before:always">Float budget specimen.</p>'
        + '<div class="left inner-edge"><p>Reed</p></div>' * 65
        + '<p class="clear-both">Anchor beyond the float budget.</p>',
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
