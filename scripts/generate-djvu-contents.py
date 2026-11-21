#!/usr/bin/env python3
"""Create and independently check the original six-page DjVu contents fixture."""
import argparse
import ast
import json
import re
import subprocess
import tempfile
from pathlib import Path
from urllib.parse import unquote

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/beacon-atlas.djvu"
PAGES = [
    ("cover.djvu", "Beacon Atlas", "Front cover"),
    ("inlet.djvu", "Inlet", "Stations"),
    ("reef-42.djvu", "Outer Reef", "Water line"),
    ("wind.djvu", "East Wind", "Wind"),
    ("cove.djvu", "Quiet Cove", "Stations"),
    ("dusk.djvu", "Dusk Watch", "Dusk"),
]
OUTLINE = """(bookmarks
 ("Front cover" "#1")
 ("Northern shore" "#inlet.djvu"
   ("Outer reef" "#reef-42.djvu")
   ("Water line" "#Water%20line")
   ("Nearest station" "#Stations"))
 ("Last watch" "#6")
 ("Next plate" "#+1")
 ("Source website" "https://example.org/atlas")
 ("Unknown plate" "#plate99.djvu")
 ("Outside the book" "#72"))
"""


def run(folder, *args):
    return subprocess.check_output(args, cwd=folder, text=True)


def create(folder):
    face = ROOT / "tests/fixtures/fonts/NotoSans-Regular.ttf"
    title = ImageFont.truetype(str(face), 38)
    label = ImageFont.truetype(str(face), 16)
    for index, (name, heading, _) in enumerate(PAGES):
        page = Image.new("1", (480, 640), 1)
        draw = ImageDraw.Draw(page)
        draw.text((38, 59), heading, font=title, fill=0)
        draw.text((40, 120), f"Beacon Atlas  /  Plate {index + 1}", font=label, fill=0)
        draw.line((40, 161, 438, 161), fill=0, width=2)
        for row in range(8):
            points = [(40 + step * 10,
                       300 + row * 12 + ((step * (index + 2) + row * 2) % 29))
                      for step in range(40)]
            draw.line(points, fill=0, width=2)
        page.save(folder / "sheet.pbm")
        run(folder, "cjb2", "-dpi", "150", "sheet.pbm", name)
    run(folder, "djvm", "-c", "book.djvu", *(p[0] for p in PAGES))
    (folder / "outline.txt").write_text(OUTLINE, encoding="utf-8")
    for index, (_, _, page_title) in enumerate(PAGES, 1):
        run(folder, "djvused", "book.djvu", "-s", "-e",
            f'select {index}; set-page-title "{page_title}"')
    run(folder, "djvused", "book.djvu", "-s", "-e", "set-outline outline.txt")
    check(folder / "book.djvu")
    return (folder / "book.djvu").read_bytes()


def check(path):
    assert run(path.parent, "djvused", path.name, "-e", "n").strip() == str(len(PAGES))
    got = run(path.parent, "djvused", path.name, "-u", "-e", "print-outline")
    assert "".join(got.split()) == "".join(OUTLINE.split())
    listing = run(path.parent, "djvused", path.name, "-u", "-e", "ls")
    assert all(name in listing and title in listing for name, _, title in PAGES)
    with tempfile.TemporaryDirectory() as work:
        for i in range(1, len(PAGES) + 1):
            output = Path(work) / f"{i}.pgm"
            run(path.parent, "ddjvu", "-format=pgm", f"-page={i}", path.name, str(output))
            with Image.open(output) as frame:
                assert frame.size == (480, 640) and frame.getextrema() == (0, 255)


def compare(path, reader):
    path = path.resolve()
    total = int(run(path.parent, "djvused", path.name, "-e", "n"))
    listing = run(path.parent, "djvused", path.name, "-u", "-e", "ls")
    pages = []
    for line in listing.splitlines():
        match = re.match(r"^\s*(\d+)\s+P\s+\d+\s+(.+?)(?: T=(.+))?$", line)
        if match:
            number, identifier, title = match.groups()
            assert int(number) == len(pages) + 1
            pages.append((identifier, title or identifier))
    assert len(pages) == total

    def target(url):
        if not url.startswith("#"):
            return None
        key = unquote(url[1:])
        for i, (identifier, _) in enumerate(pages):
            if key == identifier:
                return i
        if re.fullmatch(r"[+-]\d+", key):
            n = int(key)  # the comparison begins on the cover
            return n if 0 <= n < total else None
        for i, (_, title) in enumerate(pages):
            if key == title:
                return i
        if re.fullmatch(r"\d+", key) and 1 <= int(key) <= total:
            return int(key) - 1
        return None

    # djvused -u emits UTF-8 titles, quoted URL strings and nested bookmark nodes.
    printed = run(path.parent, "djvused", path.name, "-u", "-e", "print-outline")
    tokens = iter(re.findall(r'"(?:\\.|[^"\\])*"|[()]|bookmarks', printed))
    expected = []
    depth = -1
    for token in tokens:
        if token == "(":
            depth += 1
            label = next(tokens)
            if label == "bookmarks":
                continue
            link = next(tokens)
            title = " ".join(ast.literal_eval(label).split())
            expected.append([title, depth - 1, target(ast.literal_eval(link))])
        elif token == ")":
            depth -= 1
        else:
            raise ValueError("Unexpected token in outline")
    assert depth == -1
    result = json.loads(run(path.parent, str(reader.resolve()), path.name))
    assert result["pages"] == total
    assert len(result["entries"]) == len(expected)
    for index, (actual, wanted) in enumerate(zip(result["entries"], expected)):
        assert actual == wanted, f"Outline row {index + 1} differs"
    print(f"{path.name}: {total} pages, {len(expected)} matching rows, "
          f"{sum(row[2] is not None for row in expected)} local destinations, "
          f"{len(result['sampled'])} sampled pages painted")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate the checked-in file with DjVuLibre")
    parser.add_argument("--compare", type=Path, metavar="READER", help="compare published outlines with the Rust example")
    parser.add_argument("books", type=Path, nargs="*")
    args = parser.parse_args()
    if args.compare:
        for book in args.books or [FIXTURE]:
            compare(book, args.compare)
    elif args.check:
        check(FIXTURE)
    else:
        with tempfile.TemporaryDirectory() as work:
            FIXTURE.write_bytes(create(Path(work)))
