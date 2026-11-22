#!/usr/bin/env python3
"""Compare authored PDF image dictionaries and transforms across two normal renderer builds."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import subprocess
import zlib

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SERIALIZE = runpy.run_path(str(ROOT / "scripts/generate-fixtures.py"))["pdf_objects"]
WIDTH, HEIGHT = 864, 720


def sample(mode):
    """Independent smooth print ramps with high-frequency marks in the upper band."""
    image = Image.new(mode, (WIDTH, HEIGHT))
    # Use a single small repeating image to keep generation bounded across environments.
    colors = {
        "CMYK": lambda x, y: (x, y, 255 - y, x // 4 if y > 45 else (x // 12 % 2) * 180),
        "RGB": lambda x, y: (x, y, 255 - y),
        "L": lambda x, y: (x + y) // 2,
    }
    tile = Image.new(mode, (256, 256))
    tile.putdata([colors[mode](x, y) for y in range(256) for x in range(256)])
    image.paste(tile.resize(image.size, Image.Resampling.BILINEAR))
    buffer = io.BytesIO()
    image.save(buffer, "JPEG", quality=93, optimize=False)
    return buffer.getvalue()


def stream(attrs, data):
    return (
        f"<< {attrs} /Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"
    )


def write_pdf(path, mode, dictionary, transform):
    pdf_names = {"CMYK": "/DeviceCMYK", "RGB": "/DeviceRGB", "L": "/DeviceGray"}
    color_space = pdf_names[mode] if dictionary != "profile" else "[/ICCBased 7 0 R]"
    options = {
        "plain": "",
        "inverted": "/Decode [1 0 1 0 1 0 1 0]",
        "parameters": "/DecodeParms << /ColorTransform 0 >>",
        "soft-mask": "/SMask 6 0 R",
        "key-mask": "/Mask [0 20 0 20 0 20 0 20]",
        "profile": "",
    }
    attrs = (
        f"/Type /XObject /Subtype /Image /Width {WIDTH} /Height {HEIGHT} "
        f"/BitsPerComponent 8 /ColorSpace {color_space} /Filter /DCTDecode {options[dictionary]}"
    )
    # In PDF syntax these are the emitted x/y axes followed by the translation.
    content = ("q " + " ".join(str(n) for n in transform) + " cm /Photo Do Q").encode()
    mask = zlib.compress(bytes(range(256)) * ((WIDTH * HEIGHT + 255) // 256))
    mask_dict = f"/Type /XObject /Subtype /Image /Width {WIDTH} /Height {HEIGHT} /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode"
    profile = (
        ROOT / "vendor/hayro-interpret/assets/CGATS001Compat-v2-micro.icc"
    ).read_bytes()
    path.write_bytes(
        SERIALIZE(
            [
                "<< /Type /Catalog /Pages 2 0 R >>",
                "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1264 1606] /Resources << /XObject << /Photo 5 0 R >> >> /Contents 4 0 R >>",
                stream("", content),
                stream(attrs, sample(mode)),
                stream(mask_dict, mask),
                stream("/N 4", profile),
            ]
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("renderer", type=Path)
    parser.add_argument("full_renderer", type=Path)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    fit = (122, 0, 0, 98, 150, 500)
    cases = [
        ("ordinary", "CMYK", "plain", fit, True),
        ("quarter_turn", "CMYK", "plain", (0, 122, -98, 0, 250, 320), True),
        ("detail_in_one_axis", "CMYK", "plain", (122, 0, 0, 200, 150, 500), False),
        ("skew_detail", "CMYK", "plain", (122, 0, 148, 98, 150, 500), False),
        ("decode_array", "CMYK", "inverted", fit, False),
        ("decode_parameters", "CMYK", "parameters", fit, False),
        ("soft_mask", "CMYK", "soft-mask", fit, False),
        ("color_key_mask", "CMYK", "key-mask", fit, False),
        ("icc_based", "CMYK", "profile", fit, False),
        ("rgb", "RGB", "plain", fit, False),
        ("gray", "L", "plain", fit, False),
    ]
    results = []
    for name, mode, dictionary, transform, expect_reduced in cases:
        path = args.directory / (name + ".pdf")
        write_pdf(path, mode, dictionary, transform)
        frames = []
        for renderer, label in [
            (args.full_renderer, "full"),
            (args.renderer, "guarded"),
        ]:
            output = path.with_suffix("." + label + ".png")
            run = subprocess.run(
                [renderer.resolve(), path, "1", "1", output],
                check=True,
                capture_output=True,
                timeout=60,
                env=dict(os.environ, READER_PDF_OPEN_TIMINGS="1"),
            )
            count = run.stderr.count(b"reader_pdf_scaled_dct ")
            assert count == int(label == "guarded" and expect_reduced), (
                name,
                label,
                count,
            )
            with Image.open(output) as img:
                assert img.size == (1264, 1606)
                assert img.convert("L").getextrema()[0] < 200
            frames.append(hashlib.sha256(output.read_bytes()).hexdigest())
        identical = frames[0] == frames[1]
        if not expect_reduced:
            assert identical, name
        results.append(
            dict(
                case=name,
                used_reduced_decoder=expect_reduced,
                identical_viewport=identical,
            )
        )
    (args.directory / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{len(cases)} authored eligibility, orientation and fallback cases passed")


if __name__ == "__main__":
    main()
