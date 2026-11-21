#!/usr/bin/env python3
"""Rebuild the original Reader publication fixtures and their expected results."""

import argparse
import hashlib
import io
import json
import struct
import zlib
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZIP_STORED, ZipFile, ZipInfo


FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
EPUB_TYPE = b"application/epub+zip"
XHTML = "application/xhtml+xml"
XML = '<?xml version="1.0" encoding="UTF-8"?>\n'
OPF_PATH = "EPUB/package.opf"
NAV_PATH = "EPUB/nav.xhtml"
NCX_PATH = "EPUB/toc.ncx"
SHORE_PATH = "EPUB/chapters/shore.xhtml"
DUSK_PATH = "EPUB/chapters/dusk.xhtml"
CSS_PATH = "EPUB/styles/reader.css"
IMAGE_PATH = "EPUB/images/shore.png"
FONT = "Silkscreen-Regular.ttf"
FONT_NOTICE = "OFL-Silkscreen.txt"
FONT_PATH = f"EPUB/fonts/{FONT}"
FONT_NOTICE_PATH = f"META-INF/{FONT_NOTICE}"
FONT_SHA256 = "c845473330b94c2079ce9af01c51ac8ba2d99c24f4d14c039843bbb8e642ebd8"
FONT_NOTICE_SHA256 = "86c5e9c9382cdcc5948704fdfe60f2aa164a719746931219a42736ecd9cefbd3"
FONT_SOURCE = "https://github.com/google/fonts/tree/7e8d490955e9d2629cd3095e8dadd6fafe3c16b8/ofl/silkscreen"
REPEATED_TEXT = "The tide keeps its own time."
UNIQUE_TEXT = "The copper lantern stood beside the door."
VIETNAMESE_TEXT = "Mặt trời vừa lên, những con thuyền nhỏ rời bến."
ARABIC_TEXT = "عاد الصياد إلى الميناء عند الغروب."
CJK_TEXT = "夕暮れに小さな船が港へ戻った。"
IMAGE_ALT = "Grayscale shore marker with a hill and a dark pillar"
CHAPTERS = [(SHORE_PATH, "Reading the Shore"), (DUSK_PATH, "Returning at Dusk")]

CONTAINER = XML + f'''<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="{OPF_PATH}" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
'''

STYLE = '''body { color: #232323; line-height: 1.4; }
h1 { font-size: 160%; }
.notice { border-left: 0.18em solid #555; padding: 0.3em 0.6em; background: #e5e5e5; }
.return-link { text-decoration: underline; }
img { max-width: 100%; height: auto; }
'''

EMBEDDED_FONT_STYLE = f'''@font-face {{ font-family: "Fixture Silkscreen"; src: url("../fonts/{FONT}"); }}
.type-sample {{ font-family: "Fixture Silkscreen", monospace; }}
'''


def digest(content):
    return hashlib.sha256(content).hexdigest()


def font_asset(name, checksum):
    asset = (FIXTURES / "fonts" / name).read_bytes()
    if digest(asset) != checksum:
        raise ValueError(f"Pinned fixture font asset changed: {name}")
    return asset


def archive(entries):
    """Use fixed ZIP metadata and stored entries for byte-for-byte regeneration."""
    output = io.BytesIO()
    with ZipFile(output, "w", compression=ZIP_STORED) as book:
        for name, content in entries.items():
            info = ZipInfo(name, (2000, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            payload = content.encode("utf-8") if isinstance(content, str) else content
            book.writestr(info, payload)
    return output.getvalue()


def shore_image():
    """Draw an original grayscale shore marker as a small PNG."""
    width, height = 144, 96
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            shade = 245
            if y >= 68 + (x // 16) % 2:
                shade = 193
            elif 20 < x < 95 and y > 37 + abs(x - 57) // 2:
                shade = 211
            if 103 <= x <= 113 and 26 <= y <= 75:
                shade = 56
            if 99 <= x <= 117 and 22 <= y <= 28:
                shade = 36
            rows.append(shade)

    def chunk(kind, payload):
        checksum = zlib.crc32(kind + payload)
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", checksum)

    header = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 0)) + chunk(b"IEND", b"")


def xhtml(title, body, epub3, stylesheet=None):
    epub_namespace = ' xmlns:epub="http://www.idpf.org/2007/ops"' if epub3 else ""
    css = f'\n  <link rel="stylesheet" type="text/css" href="{stylesheet}"/>' if stylesheet else ""
    return XML + f'''<html xmlns="http://www.w3.org/1999/xhtml"{epub_namespace} xml:lang="en">
<head><title>{escape(title)}</title>{css}</head>
<body>
{body}
</body>
</html>
'''


def href(path):
    return path.removeprefix("EPUB/")


def navigation(chapters, epub3, identifier, title):
    if epub3:
        links = "\n".join(f'    <li><a href="{href(path)}">{escape(label)}</a></li>' for path, label in chapters)
        return xhtml("Contents", f'<nav epub:type="toc" id="toc">\n  <h1>Contents</h1>\n  <ol>\n{links}\n  </ol>\n</nav>', True)

    points = "\n".join(
        f'''    <navPoint id="nav-{index}" playOrder="{index}">
      <navLabel><text>{escape(label)}</text></navLabel>
      <content src="{href(path)}"/>
    </navPoint>'''
        for index, (path, label) in enumerate(chapters, start=1)
    )
    return XML + f'''<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1" xml:lang="en">
  <head>
    <meta name="dtb:uid" content="{identifier}"/>
    <meta name="dtb:depth" content="1"/>
    <meta name="dtb:totalPageCount" content="0"/>
    <meta name="dtb:maxPageNumber" content="0"/>
  </head>
  <docTitle><text>{escape(title)}</text></docTitle>
  <navMap>
{points}
  </navMap>
</ncx>
'''


def publication(title, slug, version, chapters, extra_resources=(), missing_paths=(), *, rtl=False):
    epub3 = version == 3
    identifier = f"urn:primitiveos:reader:fixture:{slug}"
    nav_path = NAV_PATH if epub3 else NCX_PATH
    nav_type = XHTML if epub3 else "application/x-dtbncx+xml"
    nav_property = ' properties="nav"' if epub3 else ""
    modified = '\n    <meta property="dcterms:modified">2026-11-20T00:00:00Z</meta>' if epub3 else ""
    toc = "" if epub3 else ' toc="nav"'
    direction = ' page-progression-direction="rtl"' if rtl else ""
    publication_resources = [(path, XHTML, content) for path, _, content in chapters] + list(extra_resources)
    items = "\n".join(
        f'    <item id="resource-{index}" href="{href(path)}" media-type="{kind}"/>'
        for index, (path, kind, _) in enumerate(publication_resources, start=1)
    )
    spine = "\n".join(f'    <itemref idref="resource-{index}"/>' for index in range(1, len(chapters) + 1))
    opf = XML + f'''<package xmlns="http://www.idpf.org/2007/opf" version="{version}.0" unique-identifier="book-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="book-id">{identifier}</dc:identifier>
    <dc:title>{escape(title)}</dc:title>
    <dc:language>en</dc:language>
    <dc:creator>PrimitiveOS</dc:creator>
    <dc:rights>Original content is CC BY-NC-SA 4.0. Bundled fonts retain their own terms.</dc:rights>{modified}
  </metadata>
  <manifest>
    <item id="nav" href="{href(nav_path)}" media-type="{nav_type}"{nav_property}/>
{items}
  </manifest>
  <spine{toc}{direction}>
{spine}
  </spine>
</package>
'''
    entries = {
        "mimetype": EPUB_TYPE,
        "META-INF/container.xml": CONTAINER,
        OPF_PATH: opf,
        nav_path: navigation([(path, label) for path, label, _ in chapters], epub3, identifier, title),
    }
    entries.update((path, content) for path, _, content in publication_resources if path not in missing_paths)
    return entries


def field_notes(version, missing_image=False):
    epub3 = version == 3
    title = f"Field Notes at Dusk (EPUB {version})"
    slug = f"field-notes-epub{version}"
    note_ref = ' epub:type="noteref"' if epub3 else ""
    note_type = ' epub:type="footnote"' if epub3 else ""
    type_sample = '\n<p class="type-sample">INK SAMPLE</p>' if epub3 else ""
    shore_body = f'''<h1 id="shore">Reading the Shore</h1>
<p>A narrow path led from the house to the beach. Linh carried a notebook and drew a line from one rock to the next.</p>
<p id="tide-a">{REPEATED_TEXT}</p>
<p class="notice">Morning observation. The water reached the first step<a id="note-ref" href="#tide-note"{note_ref}>[1]</a>.</p>
<p><img src="../images/shore.png" alt="{IMAGE_ALT}" width="144" height="96"/></p>
<p xml:lang="vi">{VIETNAMESE_TEXT}</p>
<p dir="rtl" xml:lang="ar">{ARABIC_TEXT}</p>
<p xml:lang="ja">{CJK_TEXT}</p>{type_sample}
<p id="tide-b">{REPEATED_TEXT}</p>
<p id="lantern">{UNIQUE_TEXT}</p>
<p><a href="dusk.xhtml#door">Continue at the door</a></p>
<div id="tide-note"{note_type}>
  <p>The first step is measured from the beach. <a class="return-link" href="#note-ref">Return to the observation</a></p>
</div>'''
    dusk_body = '''<h1 id="dusk">Returning at Dusk</h1>
<p id="door">At dusk the door was still open. Linh left the notebook next to the lantern and listened to the boats return.</p>
<p>The last light on the water faded before the harbor lamps came on.</p>
<p><a class="return-link" href="shore.xhtml#tide-b">Return to the second tide observation</a></p>'''
    chapters = [
        (path, label, xhtml(label, body, epub3, "../styles/reader.css"))
        for (path, label), body in zip(CHAPTERS, (shore_body, dusk_body))
    ]
    resources = [(CSS_PATH, "text/css", STYLE), (IMAGE_PATH, "image/png", shore_image())]
    if epub3:
        resources[0] = (CSS_PATH, "text/css", STYLE + EMBEDDED_FONT_STYLE)
        resources.append((FONT_PATH, "font/ttf", font_asset(FONT, FONT_SHA256)))
    if missing_image:
        slug = "missing-image"
        title = "Field Notes at Dusk (Missing Image)"
    entries = publication(title, slug, version, chapters, resources, (IMAGE_PATH,) if missing_image else ())
    if epub3:
        entries[FONT_NOTICE_PATH] = font_asset(FONT_NOTICE, FONT_NOTICE_SHA256)
    return entries


def long_chapter():
    title = "A Long Walk by the Shore"
    path = "EPUB/chapters/walk.xhtml"
    paragraphs = [f'<h1 id="start">{title}</h1>']
    locations = ("the dune", "the north inlet", "the path", "the harbor wall")
    for index in range(1, 241):
        location = locations[(index - 1) % len(locations)]
        paragraphs.append(
            f'<p id="station-{index:03}">Observation {index:03}. Linh paused at {location} and marked the changing wind. '
            'She counted the boats on the water, drew their silhouettes in the notebook, and walked to the next station.</p>'
        )
    paragraphs.append('<p id="end">End of the shore walk.</p>')
    content = xhtml(title, "\n".join(paragraphs), True)
    return publication(title, "long-chapter", 3, [(path, title, content)])


def many_chapters():
    chapters = []
    for index in range(1, 49):
        path = f"EPUB/chapters/day-{index:02}.xhtml"
        title = f"Day {index:02}"
        body = f'''<h1 id="day-{index:02}">{title}</h1>
<p>The harbor notebook records day {index:02}. Linh watched a narrow boat pass between two harbor markers, then drew the shadows on the water.</p>
<p>At the end of the day, she wrapped the notebook in a cloth and carried it home.</p>'''
        chapters.append((path, title, xhtml(title, body, True)))
    return publication("Harbor Notebook", "many-chapters", 3, chapters)


def field_expectations(version):
    return {
        "opens": True,
        "epub_version": f"{version}.0",
        "navigation": "ncx" if version == 2 else "xhtml",
        "spine": [path for path, _ in CHAPTERS],
        "toc": [label for _, label in CHAPTERS],
        "anchors": {
            f"{SHORE_PATH}#tide-a": REPEATED_TEXT,
            f"{SHORE_PATH}#tide-b": REPEATED_TEXT,
            f"{SHORE_PATH}#lantern": UNIQUE_TEXT,
            f"{DUSK_PATH}#door": "At dusk the door was still open.",
        },
        "links": [
            {"chapter": SHORE_PATH, "href": "#tide-note", "target": f"{SHORE_PATH}#tide-note"},
            {"chapter": SHORE_PATH, "href": "#note-ref", "target": f"{SHORE_PATH}#note-ref"},
            {"chapter": SHORE_PATH, "href": "dusk.xhtml#door", "target": f"{DUSK_PATH}#door"},
            {"chapter": DUSK_PATH, "href": "shore.xhtml#tide-b", "target": f"{SHORE_PATH}#tide-b"},
        ],
        "resources": [CSS_PATH, IMAGE_PATH] + ([FONT_PATH] if version == 3 else []),
        "image": {"path": IMAGE_PATH, "width": 144, "height": 96, "alt": IMAGE_ALT},
        "embedded_font_sample": "INK SAMPLE" if version == 3 else None,
        "text_samples": {"vi": VIETNAMESE_TEXT, "ar": ARABIC_TEXT, "ja": CJK_TEXT},
    }


def vertical_chapters():
    title = "Across the Harbor"
    first = ('<p>Passages from a quiet harbor.</p>'
             '<p>Inter&#173;national travel depends on good weather.</p>')
    verse = "Before the tide rose, the fishermen brought their boats back to the harbor."
    vertical = '<h1 id="start">Harbor at dusk</h1>\n<p>At the <a href="home.xhtml#landing">office</a>, we wrote a letter.</p>'
    vertical += "\n".join(f'<p id="entry-{index}">{verse}</p>' for index in range(1, 31))
    vertical = xhtml("Harbor at dusk", vertical, True, "../styles/vertical.css")
    chapters = [
        ("EPUB/chapters/start.xhtml", "At the harbor", xhtml("At the harbor", first, True)),
        ("EPUB/chapters/vertical.xhtml", "Harbor at dusk", vertical),
        ("EPUB/chapters/home.xhtml", "The landing", xhtml("The landing", '<p id="landing">We returned to the landing.</p>', True)),
    ]
    css = ("EPUB/styles/vertical.css", "text/css", "body { writing-mode: vertical-rl; }\n")
    return publication(title, "vertical-chapters", 3, chapters, [css])


def signal_guide():
    title = "Along the Waterline"
    intro = """<h1>Along the Waterline</h1>
<p>Field notebook · Early summer</p>
<p>At the inlet the contour narrows. Follow the channel until the marker
<a href="legend.xhtml#beacon"><img class="beacon" src="../images/beacon.svg" alt="Beacon key"/></a>
comes into view. Beyond it, the sand turns to slate.</p>
<p>From the bluff, the small pillar lines up with the shoulder of the hill. The sketch below shows the approach from the west.</p>
<p><img class="sketch" src="../images/shore.png" alt="Hill and beacon sketched from the west"/></p>
<p>The tide has already covered the lowest steps. On the return walk, use the same mark to find the path inland.</p>"""
    legend = """<h1 id="beacon">Beacon key</h1><p>A triangle marks the landing point.
<a href="guide.xhtml">Return to the field notebook.</a></p>"""
    mark = """<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 96 96">
<path d="M48 4L94 85H2Z" fill="#292929"/><circle cx="48" cy="58" r="11" fill="white"/></svg>"""
    css = ".beacon { height: 1.15em; vertical-align: middle } .sketch { width: 68%; height: auto }"
    chapters = [
        ("EPUB/chapters/guide.xhtml", title, xhtml(title, intro, True, "../styles/reader.css")),
        ("EPUB/chapters/legend.xhtml", "Beacon key", xhtml(title, legend, True)),
    ]
    return publication(title, "signal-guide", 3, chapters, [
        (CSS_PATH, "text/css", css), (IMAGE_PATH, "image/png", shore_image()),
        ("EPUB/images/beacon.svg", "image/svg+xml", mark),
    ])


def rtl_notebook():
    title = "Harbor notebook"
    notes = [
        ("The lamp is on the quay.", "المصباح على الرصيف."),
        ("The wind is from the east.", "تهب الريح من الشرق."),
        ("One boat returns before sunset.", "يعود قارب واحد قبل الغروب."),
        ("The sea is calm tonight.", "البحر هادئ الليلة."),
    ]
    entries = ['<h1>Harbor notebook</h1>']
    for number in range(1, 17):
        english, arabic = notes[(number - 1) % len(notes)]
        entries.extend([
            f'<h2 id="entry-{number}">Observation {number}</h2>',
            f'<p dir="rtl" xml:lang="ar">{arabic}</p>',
            f'<p>{english} <a href="key.xhtml#key">Translation key</a></p>',
        ])
    chapters = [
        ("EPUB/chapters/notes.xhtml", title, xhtml(title, "\n".join(entries), True)),
        ("EPUB/chapters/key.xhtml", "Translation key",
         xhtml("Translation key", '<h1 id="key">Translation key</h1><p>Notes from the quay.</p>', True)),
    ]
    return publication(title, "rtl-notebook", 3, chapters, rtl=True)


def tilted_scan():
    """Two original scan-like PDF pages with opposite tilts and an internal link."""
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /Resources << /Font << /F 7 0 R >> >> >>",
    ]
    # Draw about the centre of a 310 x 220 pt sheet with a known rotation of +/-8 degrees.
    for index, sine in enumerate([0.139173, -0.139173]):
        cosine = 0.990268
        dx = 155 - cosine * 155 + sine * 110
        dy = 110 - sine * 155 - cosine * 110
        commands = [f"q {cosine:.6f} {sine:.6f} {-sine:.6f} {cosine:.6f} {dx:.6f} {dy:.6f} cm",
                    "BT /F 11 Tf 1 0 0 1 30 180 Tm (Field observations) Tj ET"]
        lines = ["A breeze crossed the water early that morning.",
                 "The crew gathered beside the stone steps.",
                 "Small waves rolled against the shaded quay.",
                 "Trail notes recorded the change in the tide.",
                 "At dusk the last boat returned to the cove.",
                 "The keeper closed the notebook for the day."]
        for row, text in enumerate(lines):
            commands.append(f"BT /F 9 Tf 1 0 0 1 30 {152 - row * 19} Tm ({text}) Tj ET")
        commands.append("Q")
        data = "\n".join(commands) + "\n"
        annots = " /Annots [8 0 R]" if index == 0 else ""
        objects.extend([f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 310 220] /Contents {4 + index*2} 0 R{annots} >>",
                        f"<< /Length {len(data)} >>\nstream\n{data}endstream"])
    objects.extend(["<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
                    "<< /Subtype /Link /Rect [34 72 61 91] /Dest [5 0 R /Fit] >>"])
    buffer = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, obj in enumerate(objects, 1):
        offsets.append(len(buffer))
        buffer.extend(f"{number} 0 obj\n{obj}\nendobj\n".encode("ascii"))
    xref = len(buffer)
    buffer.extend(f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode("ascii"))
    for offset in offsets:
        buffer.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
    buffer.extend(f"trailer\n<< /Root 1 0 R /Size {len(objects) + 1} >>\nstartxref\n{xref}\n%%EOF\n".encode("ascii"))
    return bytes(buffer)


def outputs():
    epub2 = archive(field_notes(2))
    epub3 = archive(field_notes(3))
    malformed = field_notes(2)
    malformed["META-INF/container.xml"] = CONTAINER.removesuffix("</container>\n")
    records = [
        ("field-notes-epub2.epub", epub2, field_expectations(2)),
        ("field-notes-epub3.epub", epub3, field_expectations(3)),
        ("tilted-scan.pdf", tilted_scan(), {
            "opens": True, "page_count": 2, "display_baseline_degrees": [-8, 8],
            "internal_link_pages": [1, 2],
        }),
        ("long-chapter.epub", archive(long_chapter()), {
            "opens": True, "epub_version": "3.0", "spine_count": 1, "observation_count": 240,
            "anchors": {"EPUB/chapters/walk.xhtml#station-137": "Observation 137.",
                        "EPUB/chapters/walk.xhtml#end": "End of the shore walk."},
        }),
        ("many-chapters.epub", archive(many_chapters()), {
            "opens": True, "epub_version": "3.0", "spine_count": 48, "toc_count": 48,
            "first_chapter": "EPUB/chapters/day-01.xhtml", "last_chapter": "EPUB/chapters/day-48.xhtml",
        }),
        ("vertical-chapters.epub", archive(vertical_chapters()), {
            "opens": True, "epub_version": "3.0", "spine_count": 3,
            "writing_modes": ["horizontal-tb", "vertical-rl", "horizontal-tb"],
            "anchors": {"EPUB/chapters/vertical.xhtml#entry-24": "Before the tide rose"},
        }),
        ("signal-guide.epub", archive(signal_guide()), {
            "opens": True, "epub_version": "3.0", "spine_count": 2,
            "inline_image": "EPUB/images/beacon.svg", "image_link": "EPUB/chapters/legend.xhtml#beacon",
        }),
        ("rtl-notebook.epub", archive(rtl_notebook()), {
            "opens": True, "epub_version": "3.0", "spine_count": 2,
            "page_progression": "rtl", "first_chapter": "EPUB/chapters/notes.xhtml",
            "anchors": {"EPUB/chapters/key.xhtml#key": "Translation key"},
        }),
        ("missing-image.epub", archive(field_notes(2, missing_image=True)), {
            "missing_resource": IMAGE_PATH, "recovery": "Keep reading with image fallback or show an actionable error.",
        }),
        ("malformed-container.epub", archive(malformed), {
            "opens": False, "error": "Malformed EPUB container XML.",
        }),
        ("truncated.epub", epub2[:-22], {
            "opens": False, "error": "ZIP is missing the end of central directory record.",
        }),
    ]
    assets = {name: content for name, content, _ in records}
    manifest = {
        "schema": 1,
        "generator": "scripts/generate-fixtures.py",
        "original_content_license": "CC-BY-NC-SA-4.0",
        "third_party": [{
            "name": "Silkscreen Regular", "license": "OFL-1.1", "source": FONT_SOURCE,
            "source_file": f"fonts/{FONT}", "sha256": FONT_SHA256,
            "notice_file": f"fonts/{FONT_NOTICE}", "notice_sha256": FONT_NOTICE_SHA256,
            "included_in": ["field-notes-epub3.epub"], "archive_notice": FONT_NOTICE_PATH,
        }],
        "fixtures": [
            {"file": name, "sha256": digest(content), "size_bytes": len(content), "expected": expected}
            for name, content, expected in records
        ],
    }
    assets["manifest.json"] = (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Compare existing bytes without writing files.")
    args = parser.parse_args()
    for name, data in outputs().items():
        path = FIXTURES / name
        if args.check:
            if not path.is_file() or path.read_bytes() != data:
                parser.error(f"Fixture differs from the generator: {path}")
        else:
            path.write_bytes(data)


if __name__ == "__main__":
    main()
