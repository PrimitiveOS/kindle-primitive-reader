"""Build the authored EPUB used to compare forced breaks through empty boxes."""
from pathlib import Path
import sys
import zipfile

fixture = Path(__file__).resolve().parents[1] / "tests/fixtures/render-empty-breaks.epub"
output = Path(sys.argv[1]) if len(sys.argv) > 1 else fixture
container = '''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>'''
sheet = '''body,p,div,section { margin:0; padding:0; text-indent:0; text-align:left; }
.split { page-break-before:always;page-break-after:always; }
.before { page-break-before:always } .after { page-break-after:always }
.decorated { margin:.5em 0;padding:.25em;border-top:2px solid #888; }
.padding { padding:.25em 0 } .margin { margin:.5em 0 }
.border { border-top:2px solid #888 } .height { height:24px }
.background { background:#ddd } .removed { display:none }
.paint-a { margin:11px 0 13px;padding:7px 0 9px;border-top:2px solid #777;
           border-bottom:3px solid #555;background:#ddd }
.paint-b { margin:17px 0 19px;padding:3px 0 5px;border-top:4px solid #999;
           border-bottom:1px solid #aaa;background:#ccc }
.height-a { height:24px } .height-b { height:28px }'''
double = '<div class="split {0}"></div><div class="split {0}"></div>'
cases = {
    "Bare": double.format(""),
    "Padding": double.format("padding"),
    "Margin": double.format("margin"),
    "Border": double.format("border"),
    "Height": double.format("height"),
    "Background": double.format("background"),
    "Decorated": double.format("decorated"),
    "Removed": double.format("removed decorated"),
    "Nested": '<section class="before decorated"><div class="split padding"></div></section>'
              '<section class="after decorated"><div class="split padding"></div></section>',
    "OnlyBefore": '<div class="before padding"></div><div class="before padding"></div>',
    "OnlyAfter": '<div class="after padding"></div><div class="after padding"></div>',
    "Mixed": '<div class="before padding"></div><div class="after padding"></div>',
    "Between": '<div class="after padding"></div><div class="padding"></div>'
               '<div class="before padding"></div>',
    "One": '<div class="split padding"></div>',
    "Text": '<p class="split">Text centered marker</p>',
    "BareOne": '<div class="split"></div>',
    "BareThree": '<div class="split"></div>' * 3,
    "BareOnlyBefore": '<div class="before"></div>' * 2,
    "BareOnlyAfter": '<div class="after"></div>' * 2,
    "BareMixed": '<div class="before"></div><div class="after"></div>',
    "BareBetween": '<div class="after"></div><div></div><div class="before"></div>',
    "NestedBare": '<section class="before"><div class="split"></div></section>'
                  '<section class="after"><div class="split"></div></section>',
    "BareThenPadding": '<div class="split"></div><div class="split padding"></div>',
    "PaddingThenBare": '<div class="split padding"></div><div class="split"></div>',
    "PaddingAcrossBare": '<div class="after padding"></div><div></div>'
                         '<div class="before padding"></div>',
    "Paint": '<div class="split paint-a"></div><div class="split paint-b"></div>',
    "PaintAfter": '<div class="after paint-a"></div><div class="after paint-b"></div>',
    "PaintBefore": '<div class="before paint-a"></div><div class="before paint-b"></div>',
    "PaintHeight": '<div class="split paint-a height-a"></div>'
                   '<div class="split paint-b height-b"></div>',
}
manifest = "".join(
    f'<item id="{case}" href="{case}.xhtml" media-type="application/xhtml+xml"/>'
    for case in cases
)
spine = "".join(f'<itemref idref="{case}"/>' for case in cases)
opf = f'''<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-empty-break-probe</dc:identifier>
<dc:title>Empty break specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest>{manifest}</manifest><spine>{spine}</spine></package>'''
with zipfile.ZipFile(output, "w") as archive:
    def add(name, content, *, stored=False):
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
        archive.writestr(entry, content)

    add("mimetype", "application/epub+zip", stored=True)
    add("META-INF/container.xml", container)
    add("content.opf", opf)
    for case, body in cases.items():
        add(f"{case}.xhtml", f'''<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{case}</title><style type="text/css">{sheet}</style></head>
<body><p>Test {case} first marker</p>{body}<p>Test {case} last marker</p></body></html>''')
