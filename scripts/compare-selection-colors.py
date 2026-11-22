#!/usr/bin/env python3
"""Run the pinned CREngine grayscale conversion on KOReader's named palette.

The GPL reference function is read from an explicitly supplied local checkout,
compiled only for QA, and never copied into the Reader package or this repository.
Pass the 16 tone bytes to READER_SELECTION_COLOR_ORACLE for the optional Rust check.
This checks conversion, not whole-engine typography or selection compositing.
"""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--crengine", type=Path, required=True)
parser.add_argument("--base", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
for path, revision in [(args.crengine, "b32a88ffbbf505cf2ea5208c1a49002d70d3609f"),
                       (args.base, "6e4bc81af4e04f78d81677a323a780a71b29702a")]:
    actual = subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()
    assert actual == revision, (actual, revision)
source = (args.crengine / "src/lvdrawbuf.cpp").read_text()
function = re.search(r"static lUInt32 rgbToGray\( lUInt32 color \)\s*\{[^}]+\}", source).group()
palette = dict(re.findall(r'\["([a-z]+)"\]\s*=\s*"#([0-9A-Fa-f]{6})"',
                          (args.base / "ffi/blitbuffer.lua").read_text()))
names = ["red", "orange", "yellow", "green", "olive", "cyan", "blue", "purple"]
colors = ",".join("0x" + palette[name] for name in names)
harness = "#include <stdio.h>\ntypedef unsigned int lUInt32;\n" + function + "\n" + r'''
int main(void) {
    lUInt32 colors[]={COLORS};
    for (int i=0;i<8;i++) for (int night=0;night<2;night++)
        putchar(rgbToGray(night ? (colors[i]^0xFFFFFF) : colors[i]));
}
'''.replace("COLORS", colors)
with tempfile.TemporaryDirectory(prefix="reader-selection-reference-") as temp:
    path = Path(temp)
    (path / "tones.c").write_text(harness)
    subprocess.run(["cc", "-O2", str(path / "tones.c"), "-o", str(path / "tones")], check=True)
    result = subprocess.check_output([str(path / "tones")])
assert len(result) == 16
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_bytes(result)
print("Generated 16 pinned CREngine selection tones")
