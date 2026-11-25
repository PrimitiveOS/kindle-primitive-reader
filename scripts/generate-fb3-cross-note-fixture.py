#!/usr/bin/env python3
"""Reproduce the authored, cross-part FB3 in-page-note comparison fixtures.

All words and layout content are original test material.  The long middle
passage makes Reader split its converted body while KOReader reads one DOM.
"""

import argparse
from io import BytesIO
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

READER = Path(__file__).resolve().parent.parent
FIXTURES = READER / "tests/fixtures"

TYPES = '''<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/body.xml" ContentType="application/fb3-body+xml"/></Types>'''

OPENING = '''<?xml version="1.0" encoding="UTF-8"?>
<fb3-body xmlns="http://www.fictionbook.org/FictionBook3/body" xmlns:l="http://www.w3.org/1999/xlink">
<section xml:lang="en"><title><p>Outer Survey Log</p></title>
<p id="opening">The early watch sets out. <a l:href="#late">Find the final sounding</a>. <note href="later-note" l:role="footnote">Read the survey note</note>.</p>
<section id="soundings" xml:lang="en-GB"><title><p>Long Soundings</p></title>
<epigraph xml:lang="en-GB"><p>We start at the first lantern.</p>

'''
PASSAGE = '<p>The lantern remains lit during the long passage. Every marker through the harbor stays visible.</p>\n'
ENDING = '''<p id="late">The final sounding records a silver buoy. <a l:href="#opening">Return to the early watch</a>.</p>
</epigraph></section>
<section id="primitiveos-fb3-section-1"><title><p>Shore Register</p></title><p>The shore register closes at dusk.</p></section>
</section>
<notes><notebody id="later-note"><p>The note records a calm sea. <a l:href="#opening">Return to the boat</a>.</p></notebody></notes>
</fb3-body>'''

QUIET_OPENING = '''<?xml version="1.0" encoding="UTF-8"?>
<fb3-body xmlns="http://www.fictionbook.org/FictionBook3/body" xmlns:l="http://www.w3.org/1999/xlink">
<section><title><p>Field Dispatch</p></title><p id="dispatch">The quiet dispatch arrived. <note href="rain-note" l:role="footnote">Read the rain note</note>.</p></section>
<section id="filler"><title><p>Long Log</p></title>'''
QUIET_ENDING = '''</section>
<notes><notebody id="rain-note"><p>Rainfall reached six millimeters overnight. <a l:href="#dispatch">Return to dispatch</a>.</p></notebody></notes>
</fb3-body>'''

MULTI_OPENING = '''<?xml version="1.0" encoding="UTF-8"?>
<fb3-body xmlns="http://www.fictionbook.org/FictionBook3/body" xmlns:l="http://www.w3.org/1999/xlink">
<section><title><p>Field Dispatch</p></title><p id="dispatch">The dispatch has <note href="rain-note" l:role="footnote">rain one</note> and <note href="rain-note" l:role="footnote">rain two</note>.</p><p id="later">The next report has <note href="snow-note" l:role="footnote">snow note</note>.</p><p id="again">A later report repeats <note href="rain-note" l:role="footnote">rain three</note>.</p></section>
<section id="filler"><title><p>Long Log</p></title>'''
MULTI_ENDING = '''</section>
<notes><notebody id="rain-note"><p>Rainfall reached six millimeters. <a l:href="#dispatch">Return to dispatch</a>.</p></notebody><notebody id="snow-note"><p>Snowfall reached four centimeters. <a l:href="#later">Return to report</a>.</p></notebody></notes>
</fb3-body>'''


def package(body: str) -> bytes:
    output = BytesIO()
    with ZipFile(output, "w", compression=ZIP_DEFLATED, compresslevel=6) as archive:
        for name, body in (
            ("[Content_Types].xml", TYPES),
            ("body.xml", body),
        ):
            info = ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, body.encode("utf-8"), compresslevel=6)
    return output.getvalue()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="regenerate the selected tracked fixtures")
    parser.add_argument("--fixture", choices=("all", "continuation", "quiet", "multiple"), default="all")
    args = parser.parse_args()
    fixtures = {
        "continuation": ("harbor-continuation.fb3", OPENING + PASSAGE * 4290 + ENDING),
        # The middle section has exactly 393,213 text bytes, just below the
        # 384 KiB grouping budget. The short host and notes each force a
        # separate generated section without splitting any individual node.
        "quiet": ("cross-note-quiet.fb3", QUIET_OPENING + PASSAGE.rstrip("\n") * 4139 + QUIET_ENDING),
        "multiple": ("cross-note-multiple.fb3", MULTI_OPENING + PASSAGE.rstrip("\n") * 4139 + MULTI_ENDING),
    }
    for key, (name, body) in fixtures.items():
        if args.fixture not in ("all", key):
            continue
        target = FIXTURES / name
        data = package(body)
        if args.write:
            target.write_bytes(data)
            print(f"Wrote {name}: {len(data)} bytes")
        elif target.read_bytes() == data:
            print(f"Verified {name}: {len(data)} bytes")
        else:
            raise SystemExit(f"{name} differs from the generated package")


if __name__ == "__main__":
    main()
