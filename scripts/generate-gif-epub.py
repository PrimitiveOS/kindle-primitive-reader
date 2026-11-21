#!/usr/bin/env python3
"""Regenerate the original semaphore diagram used by the EPUB GIF checks (Pillow 12.1)."""
from io import BytesIO
from pathlib import Path
from runpy import run_path

from PIL import Image, ImageDraw

repo = Path(__file__).resolve().parents[1]
epub = run_path(str(repo / "scripts/generate-fixtures.py"))
width, height = 384, 256
art = Image.new("P", (width, height), 0)
art.putpalette([255, 255, 255, 29, 29, 29, 105, 112, 117, 179, 185, 193, 222, 226, 228] + [0] * (768 - 15))
pen = ImageDraw.Draw(art)
pen.rounded_rectangle((7, 7, width - 8, height - 8), radius=18, fill=4, outline=2, width=3)
pen.line((22, 218, 362, 218), fill=2, width=2)
for x, left, right in [(100, (46, 49), (157, 85)), (281, (224, 93), (334, 36))]:
    pen.line((x - 13, 214, x, 162, x + 14, 214), fill=1, width=9)
    pen.line((x, 107, x, 165), fill=1, width=11)
    pen.ellipse((x - 10, 83, x + 10, 103), fill=1)
    for end in [left, right]:
        pen.line((x, 118, *end), fill=1, width=5)
        fx, fy = end
        pen.polygon([(fx, fy), (fx + 26, fy - 6), (fx + 25, fy + 17), (fx, fy + 23)], fill=0, outline=1, width=2)
        pen.polygon([(fx, fy), (fx + 26, fy - 6), (fx, fy + 23)], fill=2)
encoded = BytesIO()
art.save(encoded, format="GIF", transparency=0, optimize=False, interlace=True)
gif = encoded.getvalue()
second = art.copy()
ImageDraw.Draw(second).rectangle((30, 30, 350, 210), fill=1)
animation = BytesIO()
art.save(animation, format="GIF", append_images=[second], save_all=True, duration=[250, 250], loop=0, transparency=0, optimize=False)
svg = '<svg xmlns="http://www.w3.org/2000/svg" width="384" height="256"><rect width="384" height="256" fill="#888"/><image href="flags.gif" width="384" height="256"/></svg>'
def chapter(key, name, body):
    return (f"EPUB/chapters/{key}.xhtml", name, epub["xhtml"](name, body, True))
chapters = [
    chapter("signal", "Harbor signals", '<h1>Harbor signals</h1><p id="station">From the east pier, a pair of flags carries a message across the water.</p><p><img src="../art/flags.gif" alt="Two semaphore signals" style="width:85%;height:auto"/></p><p>Read the two positions from left to right.</p><p><a href="../art/flags.gif">Open the signal chart</a></p>'),
    chapter("layer", "Viewing the flags", '<p id="layer">A shaded background shows the cutouts in each flag.</p>' + svg.replace('href="flags.gif"', 'href="../art/flags.gif"') + '<p><img src="../art/layer.svg" alt="The same diagram inside a separate SVG"/></p>'),
    chapter("change", "Signal change", '<p id="change">The first signal appears in the book.</p><p><img src="../art/change.gif" alt="Changing signal"/></p>'),
]
entries = epub["publication"]("Harbor signals", "gif-signals", 3, chapters, [
    ("EPUB/art/flags.gif", "image/gif", gif), ("EPUB/art/layer.svg", "image/svg+xml", svg),
    ("EPUB/art/change.gif", "image/gif", animation.getvalue())])
entries[epub["OPF_PATH"]] = entries[epub["OPF_PATH"]].replace('href="art/flags.gif"','href="art/flags.gif" properties="cover-image"')
target = repo / "tests/fixtures/gif-signals.epub"
target.write_bytes(epub["archive"](entries))
