#!/usr/bin/env python3
"""Generate the large print and fine-line chart used by the optional DCT benchmark."""
import argparse
import io
import math
import runpy
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def image_bytes(font_path):
    image = Image.new("CMYK", (3504, 4608), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    black = (0, 0, 0, 255)
    for index, point_size in enumerate((20, 24, 32, 40, 52, 82)):
        font = ImageFont.truetype(str(font_path), point_size)
        for row in range(4):
            y = index * 460 + 82 + point_size * 1.4 * row
            text = f"Measure {index + 1}: Reading small print. Ink 0123456789 – é à ü."
            draw.text((84, y), text, fill=black, font=font)
    for row in range(4):
        for column in range(24):
            ink = int(255 * column / 23)
            color = (ink, 72 * row, 255 - ink, 255 - 72 * row)
            left, top = 75 + column * 137, 2890 + 165 * row
            draw.rectangle((left, top, left + 131, top + 135), fill=color)
    for period in (2, 4, 8, 16, 32):
        top = 3660 + 72 * int(math.log2(period))
        for start in range(82, image.width - 82, period * 2):
            draw.rectangle((start, top, start + period - 1, top + 55), fill=black)
    for line in range(125):
        draw.line((2600 + line * 4, 4300, 2250 + line * 8, 4511), fill=black, width=2)
    # Pillow encodes inverted Adobe CMYK planes. The PDF uses the default Decode array.
    image = image.point(list(range(255, -1, -1)) * 4)
    encoded = io.BytesIO()
    image.save(encoded, "JPEG", quality=90)
    return encoded.getvalue()


def stream(dictionary, data):
    return b"<< " + dictionary + f" /Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    scripts = Path(__file__).resolve().parent
    font = scripts.parent / "tests/fixtures/fonts/NotoSans-Regular.ttf"
    content = (b"q 600 0 0 780 475 100 cm /Photo Do Q "
               b"BT /F 12 Tf 475 890 Td (Printed scan and grayscale control) Tj ET")
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1400 1000] /Contents 4 0 R "
        "/Resources << /Font << /F 6 0 R >> /XObject << /Photo 5 0 R >> >> >>",
        stream(b"", content),
        stream(b"/Type /XObject /Subtype /Image /Width 3504 /Height 4608 "
               b"/BitsPerComponent 8 /ColorSpace /DeviceCMYK /Filter /DCTDecode", image_bytes(font)),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    serialize = runpy.run_path(str(scripts / "generate-fixtures.py"))["pdf_objects"]
    args.output.write_bytes(serialize(objects))


if __name__ == "__main__":
    main()
