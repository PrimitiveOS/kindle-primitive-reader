"""Build authored two-page PDF controls specimens and the raster page-2 OCR record."""

from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import zlib

DESTINATION = Path(__file__).resolve().parents[1] / "tests/fixtures/scan-reflow"
LINES = [
    "Harbor notes from the eastern shore",
    "Take a reading at the first deep marker and record its height",
    "Watch the shallow channel at low tide and follow the darker water",
    "The short route stays close to shore when the weather is calm",
    "Some rocks lie just beyond the outer beacon and its white light",
    "Use the shelf on the north side to wait for the passing boats",
    "The tide turns slowly and rises for several hours before dawn",
    "Keep a safe distance between the rocks and the wooden pier",
    "Return by the marked passage and record the time of the crossing",
    "Compare these notes with the earlier chart before leaving the bay",
]


def write_pdf(filename, objects):
    data = bytearray(b"%PDF-1.4\n")
    offsets = []
    for index, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data += f"{index} 0 obj\n".encode() + obj + b"\nendobj\n"
    xref = len(data)
    data += f"xref\n0 {len(offsets) + 1}\n0000000000 65535 f \n".encode()
    data += b"".join(f"{offset:010d} 00000 n \n".encode() for offset in offsets)
    data += (
        f"trailer\n<< /Size {len(offsets) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF\n"
    ).encode()
    filename.write_bytes(data)


def stream(payload, description=""):
    return (
        f"<< /Length {len(payload)} {description} >>\nstream\n".encode()
        + payload
        + b"\nendstream"
    )


def text_page(lines, size, first, pitch):
    return b"0 g\n" + b"".join(
        f"BT /F1 {size} Tf 45 {first - index * pitch} Td ({line}) Tj ET\n".encode()
        for index, line in enumerate(lines)
    )


def page(index, resource):
    return (
        f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] "
        f"/Contents {index + 4} 0 R /Resources {resource} >>"
    ).encode()


def main():
    DESTINATION.mkdir(parents=True, exist_ok=True)
    root = b"<< /Type /Catalog /Pages 2 0 R >>"
    pages = b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>"
    font = "<< /Font << /F1 7 0 R >> >>"
    long_lines = [
        f"Marker {index + 1}: {LINES[index % len(LINES)]} and record the afternoon tide."
        for index in range(57)
    ]
    vector = DESTINATION / "vector.pdf"
    write_pdf(vector, [
        root, pages, page(1, font), page(2, font),
        stream(zlib.compress(text_page(LINES, 14, 700, 25)), "/Filter /FlateDecode"),
        stream(zlib.compress(text_page(long_lines, 9, 742, 12)), "/Filter /FlateDecode"),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ])

    with TemporaryDirectory() as scratch:
        images = []
        for index in (1, 2):
            raster = Path(scratch) / f"page-{index}"
            # Match Reader's 1800-pixel OCR raster for a 600 × 800-point PDF.
            subprocess.run([
                "pdftoppm", "-gray", "-r", "162", "-f", str(index), "-l", str(index),
                "-singlefile", str(vector), str(raster),
            ], check=True)
            pgm = raster.with_suffix(".pgm")
            magic, size, depth, gray = pgm.read_bytes().split(b"\n", 3)
            assert (magic, size, depth) == (b"P5", b"1350 1800", b"255")
            images.append(stream(
                zlib.compress(gray), "/Type /XObject /Subtype /Image /Width 1350 "
                "/Height 1800 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode",
            ))
            if index == 2:
                tsv = subprocess.check_output(
                    ["tesseract", str(pgm), "stdout", "-l", "eng", "--psm", "6", "tsv"]
                )
                (DESTINATION / "page-2-ocr.tsv").write_bytes(
                    b"\n".join(row.rstrip(b"\t ") for row in tsv.splitlines()) + b"\n"
                )
        placement = stream(b"q 600 0 0 800 0 0 cm /IM Do Q")
        write_pdf(DESTINATION / "raster.pdf", [
            root, pages, page(1, "<< /XObject << /IM 7 0 R >> >>"),
            page(2, "<< /XObject << /IM 8 0 R >> >>"), placement, placement, *images,
        ])


if __name__ == "__main__":
    main()
