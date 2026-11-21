#!/usr/bin/env python3
"""Verify or regenerate Reader's original lossless JPEG 2000 image and comic fixtures."""

import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
WIDTH, HEIGHT = 160, 112


def beacon(x, y):
    """A dark coastal beacon against a pale sky, water and reflected light."""
    if (x - 111) ** 2 + (y - 30) ** 2 <= 12 ** 2:
        return (250,)
    if 11 <= x <= 50 and y >= 91 - x // 3:
        return (33,)
    if (31 <= x <= 38 and 32 <= y < 82) or (27 <= x <= 42 and 27 <= y < 32):
        return (24,)
    if 30 <= x <= 39 and 21 <= y < 27:
        return (242,)
    if 28 <= x <= 41 and abs(x - 34) <= y - 13 and y < 21:
        return (33,)
    if y < 73:
        return (124 + y,)
    return (155 if x in range(100, 122) and (x + y) % 8 < 3 else 61 + (3 * y + x // 17) % 35,)


def flags(x, y):
    """Three flat maritime signal flags hanging from a sloped line."""
    line = 19 + x // 9
    if abs(y - line) <= 1:
        return (28, 40, 45)
    index = x // 51
    if index < 3 and 8 <= x % 51 < 45 and line < y < line + 48:
        if index == 0:
            return (235, 172, 49) if (x % 51) < 26 else (34, 69, 93)
        if index == 1:
            return (174, 48, 41) if abs(x % 51 - 25) < 6 or abs(y - line - 24) < 6 else (247, 241, 217)
        return (83, 125, 76) if (x % 51 + y - line) % 27 < 13 else (218, 222, 177)
    return (195 + y // 8, 210 + y // 9, 223 + y // 12)


SAMPLES = [
    ("harbor-beacon.jp2", "pgm", beacon, []),
    ("signal-flags.j2k", "ppm", flags, ["-t", "80,56"]),
]


def pnm_pixels(path, max_value=255):
    with path.open("rb") as file:
        tokens = []
        while len(tokens) < 4:
            tokens.extend(file.readline().partition(b"#")[0].split())
        if tokens[1:] != [str(WIDTH).encode(), str(HEIGHT).encode(), str(max_value).encode()]:
            raise ValueError(f"Unexpected OpenJPEG output header in {path.name}")
        return file.read()


def run(command):
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def high_depth(work, write):
    name = "evening-light.jp2"
    values = [(beacon(x, y)[0] << 8) | ((x + 3*y) % 256)
              for y in range(HEIGHT) for x in range(WIDTH)]
    raw = b"".join(value.to_bytes(2, "big") for value in values)
    path = work / "high-depth.pgm"
    path.write_bytes(f"P5\n{WIDTH} {HEIGHT}\n65535\n".encode() + raw)
    if write:
        run(["opj_compress", "-i", str(path), "-o", str(ROOT / name),
             "-n", "4", "-C", "PrimitiveOS original fixture"])
    output = work / "decoded16.pgm"
    run(["opj_decompress", "-i", str(ROOT / name), "-o", str(output)])
    if pnm_pixels(output, 65535) != raw:
        raise ValueError("OpenJPEG did not reproduce the original 16-bit grayscale samples")
    reference = bytes((sample*255 + 32767)//65535 for sample in values)
    print(f"{name}: unsigned 16-bit; grayscale MD5 {hashlib.md5(reference).hexdigest()}")


def chroma_sample(work, write):
    name = "harbor-lights.jp2"
    luminance = bytes(75 + (x//2 + y//3) % 111 for y in range(HEIGHT) for x in range(WIDTH))
    blue = bytes(108 + (x + y) % 35 for y in range(HEIGHT//2) for x in range(WIDTH//2))
    red = bytes(117 + (x//2 + y) % 39 for y in range(HEIGHT//2) for x in range(WIDTH//2))
    if write:
        path = work / "lights.raw"
        path.write_bytes(luminance + blue + red)
        run(["opj_compress", "-i", str(path), "-o", str(ROOT / name), "-F",
             f"{WIDTH},{HEIGHT},3,8,u@1x1:2x2:2x2", "-mct", "0", "-n", "4",
             "-I", "-r", "7", "-C", "PrimitiveOS original fixture"])
    output = work / "decoded-chroma.ppm"
    run(["opj_decompress", "-i", str(ROOT / name), "-o", str(output)])
    rgb = pnm_pixels(output)
    reference = bytes((2126*r + 7152*g + 722*b)//10000
                      for r, g, b in zip(rgb[::3], rgb[1::3], rgb[2::3]))
    gray = ROOT / "harbor-lights-reference.pgm"
    expected = f"P5\n{WIDTH} {HEIGHT}\n255\n".encode() + reference
    if write:
        gray.write_bytes(expected)
    elif expected != gray.read_bytes():
        raise ValueError("Stored reference differs from the OpenJPEG sYCC conversion")
    print(f"{name}: lossy 4:2:0; grayscale MD5 {hashlib.md5(reference).hexdigest()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="replace the generated fixtures")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for name, kind, pixel, options in SAMPLES:
            raw = bytes(c for y in range(HEIGHT) for x in range(WIDTH) for c in pixel(x, y))
            plain = work / f"source.{kind}"
            plain.write_bytes((f"{'P5' if kind == 'pgm' else 'P6'}\n{WIDTH} {HEIGHT}\n255\n").encode() + raw)
            if args.write:
                encoded = work / name
                run(["opj_compress", "-i", str(plain), "-o", str(encoded), "-n", "4",
                     "-C", "PrimitiveOS original fixture", *options])
                shutil.copyfile(encoded, ROOT / name)
            decoded = work / f"decoded.{kind}"
            run(["opj_decompress", "-i", str(ROOT / name), "-o", str(decoded)])
            reference = pnm_pixels(decoded)
            if reference != raw:
                raise ValueError(f"OpenJPEG decoded pixels differ from the source of {name}")
            if kind == "ppm":
                reference = bytes((2126*r + 7152*g + 722*b)//10000
                                  for r, g, b in zip(raw[::3], raw[1::3], raw[2::3]))
            print(f"{name}: {WIDTH} × {HEIGHT}; grayscale MD5 {hashlib.md5(reference).hexdigest()}")
        high_depth(work, args.write)
        chroma_sample(work, args.write)
        comic = work / "harbor-signals.cbz"
        with zipfile.ZipFile(comic, "w") as archive:
            for i, (name, _, _, _) in enumerate(SAMPLES, 1):
                archive.writestr(zipfile.ZipInfo(f"{i:02}-{name}", (2000, 1, 1, 0, 0, 0)),
                                 (ROOT / name).read_bytes())
        target = ROOT / comic.name
        if args.write:
            shutil.copyfile(comic, target)
        elif target.read_bytes() != comic.read_bytes():
            raise ValueError("Comic contents differ from the standalone images")


if __name__ == "__main__":
    main()
