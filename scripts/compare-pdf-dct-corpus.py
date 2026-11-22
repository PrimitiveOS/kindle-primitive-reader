#!/usr/bin/env python3
"""Measure reduced JPEG decoding on pinned, independently sourced print fixtures."""
import argparse
import hashlib
import io
import json
import math
import os
from pathlib import Path
import subprocess
import urllib.request

import numpy as np
from PIL import Image
import pymupdf


ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 1264, 1606
LIMIT = 16 * 1024 * 1024
# Fix the views and numerical criteria before inspecting any test results.
VIEWS = ("fit", "crop", "detail")
CRITERIA = {
    "mae_max": 0.75,
    "p99_max": 8,
    "ssim_min": 0.995,
    "over_one_shade_percent_max": 0.01,
    "reference_ssim_loss_max": 0.003,
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_fixture(source, directory):
    if not source["url"].startswith(
        ("https://raw.githubusercontent.com/", "https://media.githubusercontent.com/")
    ):
        raise ValueError("Expected a pinned GitHub source")
    target = directory / source["sha256"]
    if not target.exists():
        request = urllib.request.Request(
            source["url"], headers={"User-Agent": "PrimitiveOS-Reader-DCT-comparison"}
        )
        with urllib.request.urlopen(request, timeout=60) as connection:
            data = connection.read(LIMIT + 1)
        if len(data) > LIMIT or digest(data) != source["sha256"]:
            raise ValueError(f"Source checksum or size mismatch for {source['id']}")
        target.write_bytes(data)
    data = target.read_bytes()
    if digest(data) != source["sha256"]:
        raise ValueError("Cached corpus file changed")
    decode = ""
    if "xref" in source:
        with pymupdf.open(stream=data, filetype="pdf") as doc:
            xref = source["xref"]
            assert doc.xref_get_key(xref, "Filter") == ("name", "/DCTDecode")
            assert doc.xref_get_key(xref, "ColorSpace") == ("name", "/DeviceCMYK")
            assert doc.xref_get_key(xref, "BitsPerComponent") == ("int", "8")
            for key in ("SMask", "Mask", "DecodeParms"):
                assert doc.xref_get_key(xref, key)[0] == "null"
            kind, value = doc.xref_get_key(xref, "Decode")
            if kind == "array":
                numbers = [float(n) for n in value.strip("[]").split()]
                assert len(numbers) == 8 and all(n in (0.0, 1.0) for n in numbers)
                decode = "/Decode [" + " ".join(str(int(n)) for n in numbers) + "]"
            else:
                assert kind == "null"
            data = doc.xref_stream_raw(xref)
    with Image.open(io.BytesIO(data)) as image:
        if image.mode != "CMYK":
            raise ValueError("Corpus entry is not a CMYK or YCCK JPEG")
        return data, image.size, image.info.get("adobe_transform"), decode


def geometry(size, view):
    w, h = size
    factor = {"fit": 8, "crop": 6.4, "detail": 3}[view]
    dw, dh = w / factor, h / factor
    # Crop against the page edges so the renderer performs real source clipping.
    x, y = (
        (-0.31 * dw, -0.19 * dh)
        if view == "crop"
        else ((WIDTH - dw) / 2, (HEIGHT - dh) / 2)
    )
    region = (
        max(0, math.ceil(x) + 1),
        max(0, math.ceil(HEIGHT - y - dh) + 1),
        min(WIDTH, math.floor(x + dw) - 1),
        min(HEIGHT, math.floor(HEIGHT - y) - 1),
    )
    return (x, y, dw, dh), region


def write_document(path, jpeg, size, decode):
    w, h = size
    objects = []

    def add(data):
        objects.append(data if isinstance(data, bytes) else data.encode("ascii"))

    add("<< /Type /Catalog /Pages 2 0 R >>")
    add("<< /Type /Pages /Count 3 /Kids [4 0 R 6 0 R 8 0 R] >>")
    add(
        f"<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /BitsPerComponent 8 "
        f"/ColorSpace /DeviceCMYK /Filter /DCTDecode {decode} /Length {len(jpeg)} >>\nstream\n".encode()
        + jpeg
        + b"\nendstream"
    )
    for index, view in enumerate(VIEWS):
        add(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] "
            f"/CropBox [0 0 {WIDTH} {HEIGHT}] /Resources << /XObject << /I 3 0 R >> >> /Contents {5+2*index} 0 R >>"
        )
        (x, y, dw, dh), _ = geometry(size, view)
        commands = f"q {dw:.8f} 0 0 {dh:.8f} {x:.8f} {y:.8f} cm /I Do Q\n".encode()
        add(
            f"<< /Length {len(commands)} >>\nstream\n".encode()
            + commands
            + b"endstream"
        )
    pdf = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf.extend(f"{i} 0 obj\n".encode() + obj + b"\nendobj\n")
    start = len(pdf)
    pdf.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for o in offsets[1:]:
        pdf.extend(f"{o:010d} 00000 n \n".encode())
    pdf.extend(
        f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n".encode()
    )
    path.write_bytes(pdf)


def metrics(a, b):
    error = np.abs(a - b)
    h, w = a.shape
    # Small exact-fallback controls may have fewer than eight pixels in one dimension.
    block = min(8, h, w)

    def means(data):
        shape = (h // block, block, w // block, block)
        return (
            data[: shape[0] * block, : shape[2] * block]
            .reshape(shape)
            .mean(axis=(1, 3))
        )

    m, n = means(a), means(b)
    va, vb = means(a * a) - m * m, means(b * b) - n * n
    co = means(a * b) - m * n
    ssim = ((2 * m * n + 6.5025) * (2 * co + 58.5225)) / (
        (m * m + n * n + 6.5025) * (va + vb + 58.5225)
    )
    shades = np.abs(np.rint(a / 17) - np.rint(b / 17))
    return {
        "mae": float(error.mean()),
        "p99": float(np.percentile(error, 99)),
        "ssim": float(ssim.mean()),
        "over_one_shade_percent": 100 * float((shades > 1).mean()),
        "max_shades": int(shades.max()),
        "pixels": int(a.size),
    }


def evaluate(renderer, full_renderer, root, source):
    jpeg, size, adobe, decode = read_fixture(source, root)
    fingerprint = digest(jpeg)
    case = root / fingerprint[:16]
    case.mkdir(exist_ok=True)
    doc = case / "case.pdf"
    write_document(doc, jpeg, size, decode)
    results = []
    with pymupdf.open(doc) as pdf:
        for index, view in enumerate(VIEWS):
            _, region = geometry(size, view)
            x, y, x2, y2 = region
            pixmap = pdf[index].get_pixmap(colorspace=pymupdf.csGRAY, alpha=False)
            reference = (
                np.frombuffer(pixmap.samples, dtype=np.uint8)
                .reshape(HEIGHT, WIDTH)[y:y2, x:x2]
                .astype(np.float64)
            )
            # Encode labels from immutable input, then compare A/B before resolving the guards.
            order = [0, 3] if int(fingerprint[index], 16) % 2 else [3, 0]
            captures = {}
            reduced = {}
            file_hash = {}
            for alias, guard in zip(("A", "B"), order):
                output = case / f"{view}-{alias}.png"
                env = dict(
                    os.environ,
                    READER_PDF_OPEN_TIMINGS="1",
                )
                run = subprocess.run(
                    [
                        renderer if guard else full_renderer,
                        doc,
                        str(index + 1),
                        "1",
                        output,
                    ],
                    env=env,
                    check=True,
                    capture_output=True,
                    timeout=360,
                )
                reduced[alias] = run.stderr.count(b"reader_pdf_scaled_dct ")
                with Image.open(output) as image:
                    assert image.size == (WIDTH, HEIGHT)
                    captures[alias] = np.asarray(
                        image.convert("L").crop(region), dtype=np.float64
                    )
                file_hash[alias] = digest(output.read_bytes())
            delta = metrics(captures["A"], captures["B"])
            full, candidate = ("A", "B") if order[0] == 0 else ("B", "A")
            assert reduced[full] == 0
            identical = file_hash[full] == file_hash[candidate]
            stock = {
                k: metrics(reference, captures[k])["ssim"] for k in (full, candidate)
            }
            loss = stock[full] - stock[candidate]
            acceptable = (
                delta["mae"] <= CRITERIA["mae_max"]
                and delta["p99"] <= CRITERIA["p99_max"]
                and delta["ssim"] >= CRITERIA["ssim_min"]
                and delta["over_one_shade_percent"]
                <= CRITERIA["over_one_shade_percent_max"]
                and loss <= CRITERIA["reference_ssim_loss_max"]
            )
            if not reduced[candidate] and not identical:
                raise ValueError("Expected byte-identical full decoder fallback")
            results.append(
                dict(
                    view=view,
                    reduced=reduced[candidate],
                    whole_frame_identical=identical,
                    delta=delta,
                    full_to_mupdf_ssim=stock[full],
                    reduced_to_mupdf_ssim=stock[candidate],
                    reference_ssim_loss=loss,
                    within_criteria=acceptable,
                )
            )
    return dict(
        source=source["id"],
        source_sha256=source["sha256"],
        jpeg_sha256=fingerprint,
        width=size[0],
        height=size[1],
        adobe_transform=adobe,
        explicit_decode=bool(decode),
        views=results,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("renderer", type=Path)
    parser.add_argument("full_renderer", type=Path)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    root = args.directory.resolve()
    root.mkdir(parents=True, exist_ok=True)
    sources = json.loads((ROOT / "tests/fixtures/dct/public-sources.json").read_text())
    results = []
    for source in sorted(sources, key=lambda s: s["sha256"]):
        result = evaluate(
            args.renderer.resolve(), args.full_renderer.resolve(), root, source
        )
        results.append(result)
        print(
            source["id"],
            [(v["view"], v["reduced"], v["within_criteria"]) for v in result["views"]],
            flush=True,
        )
    report = dict(
        criteria=CRITERIA,
        source_count=len(results),
        pymupdf=pymupdf.VersionBind,
        renderer_sha256=digest(args.renderer.read_bytes()),
        full_renderer_sha256=digest(args.full_renderer.read_bytes()),
        results=results,
    )
    (root / "results.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
