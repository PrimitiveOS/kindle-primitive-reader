#!/usr/bin/env python3
"""Verify the pinned expression engine and redistribution notices in a staged package ZIP."""
from hashlib import sha256
from pathlib import Path
import argparse
from zipfile import ZipFile

root = Path(__file__).resolve().parents[1]
header_hash = '80c2d6436d76822ef8722a7132845e6d9e0231bc4f90b57e5999a608b9aa9101'
notice_hash = '013ba4e9f5ba60c72efa40f638d09b3d1a72c9a4787560207179710e1060d3be'
symbol_font_hash = '3887de549fa590384893229e24302ebad818cedb659632978c21e26047999121'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('package', type=Path)
args = parser.parse_args()
if sha256((root / 'vendor/srell/srell.hpp').read_bytes()).hexdigest() != header_hash:
    raise SystemExit('Pinned SRELL header differs')
with ZipFile(args.package) as package:
    # A release wraps the staged tree in a versioned directory.
    roots = [name[:-len('reader/licenses/SRELL.txt')] for name in package.namelist()
             if name == 'reader/licenses/SRELL.txt' or name.endswith('/reader/licenses/SRELL.txt')]
    if len(roots) != 1:
        raise SystemExit('Expected exactly one Reader package root')
    for file in ['SRELL.txt', 'ocr/libcxx-LICENSE', 'ocr/libcxxabi-LICENSE', 'ocr/libunwind-LICENSE']:
        data = package.read(roots[0] + 'reader/licenses/' + file)
        if data != (root / 'docs/licenses' / file).read_bytes():
            raise SystemExit('Bundled license differs: ' + file)
        if file == 'SRELL.txt' and sha256(data).hexdigest() != notice_hash:
            raise SystemExit('SRELL notice differs from upstream')
    assets = {
        'fonts/PrimitiveSymbols.ttf': root / 'assets/fonts/PrimitiveSymbols.ttf',
        'licenses/NERDFONTS.txt': root / 'docs/licenses/NERDFONTS.txt',
        'licenses/HYPHENATION-RU.txt': root / 'docs/licenses/HYPHENATION-RU.txt',
        'licenses/HYPHENATION-SV.txt': root / 'docs/licenses/HYPHENATION-SV.txt',
        'licenses/LPPL-1.3c.txt': root / 'docs/licenses/LPPL-1.3c.txt',
        'licenses/hyph-ru.tex': root / 'assets/hyphenation/hyph-ru.tex',
        'licenses/hyph-sv.tex': root / 'assets/hyphenation/hyph-sv.tex',
    }
    for file, source in assets.items():
        data = package.read(roots[0] + 'reader/' + file)
        if data != source.read_bytes():
            raise SystemExit('Bundled Reader asset differs: ' + file)
        if file == 'fonts/PrimitiveSymbols.ttf' and sha256(data).hexdigest() != symbol_font_hash:
            raise SystemExit('Nerd Fonts symbol asset differs from its reviewed upstream copy')
print('Verified the pinned SRELL header, binary notices and MIT symbol fallback font.')
