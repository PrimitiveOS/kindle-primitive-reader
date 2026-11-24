"""Package the authored inline-box layout specimen for the native replay."""

from pathlib import Path
import sys
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile, ZipInfo

source = Path(__file__).resolve().parents[1] / "tests/fixtures/render-inline-blocks.xhtml"
with ZipFile(sys.argv[1], "w") as book:
    for name, text in {
        "mimetype": "application/epub+zip",
        "META-INF/container.xml": """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>""",
        "content.opf": """<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-inline-boxes</dc:identifier>
<dc:title>Inline boxes specimen</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="sample" href="sample.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="sample"/></spine>
</package>""",
        "sample.xhtml": source.read_text(encoding="utf-8"),
    }.items():
        entry = ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = ZIP_STORED if name == "mimetype" else ZIP_DEFLATED
        book.writestr(entry, text)
