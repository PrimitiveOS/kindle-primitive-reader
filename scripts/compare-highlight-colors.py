#!/usr/bin/env python3
"""Generate coverage from an explicit, pinned KOReader blitbuffer checkout.

The reference C implementation is a local QA input and is not packaged in Reader.
Pass the output to READER_COLOR_ORACLE for the optional Rust comparison test.
"""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

PIN = "6e4bc81af4e04f78d81677a323a780a71b29702a"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--reference", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
reference = args.reference.resolve()
assert subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip() == PIN
colors = dict(re.findall(r'\["([a-z]+)"\]\s*=\s*"#([0-9A-Fa-f]{6})"', (reference / "ffi/blitbuffer.lua").read_text()))
names = ["red", "orange", "yellow", "green", "olive", "cyan", "blue", "purple"]
triples = [[int(colors[name][index:index+2], 16) for index in (0, 2, 4)] for name in names]
initializers = ",".join("{" + ",".join(map(str, rgb)) + "}" for rgb in triples)
harness = r'''
#include <stdio.h>
#include "blitbuffer.h"
int main(void) {
    ColorRGB24 colors[]={COLORS};
    unsigned char pixels[256];
    BlitBuffer bb={256,256,1,256,pixels,TYPE_BB8<<SHIFT_TYPE};
    for(int c=0;c<8;c++) {
        for(int i=0;i<256;i++)pixels[i]=i;
        BB_blend_RGB_multiply_rect(&bb,0,0,256,1,&colors[c]);fwrite(pixels,1,256,stdout);
        ColorRGB32 line={colors[c].r,colors[c].g,colors[c].b,255};
        BB_fill_rect_RGB32(&bb,0,0,256,1,&line);fwrite(pixels,1,256,stdout);
        for(int percent=0;percent<=100;percent+=5) {
            for(int i=0;i<256;i++)pixels[i]=i;
            ColorRGB32 night={255-colors[c].r,255-colors[c].g,255-colors[c].b,percent*255/100};
            BB_blend_RGB32_over_rect(&bb,0,0,256,1,&night);fwrite(pixels,1,256,stdout);
        }
    }
}
'''.replace("COLORS", initializers)
with tempfile.TemporaryDirectory(prefix="reader-color-reference-") as temp:
    folder = Path(temp)
    source = folder / "compare.c"
    source.write_text(harness)
    executable = folder / "compare"
    subprocess.run(["cc", "-O2", "-I", str(reference), str(source), str(reference / "blitbuffer.c"), "-o", str(executable)], check=True)
    output = subprocess.check_output([str(executable)])
assert len(output) == 8 * 23 * 256
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_bytes(output)
print(f"Generated {len(output)} coverage bytes from koreader-base {PIN}")
