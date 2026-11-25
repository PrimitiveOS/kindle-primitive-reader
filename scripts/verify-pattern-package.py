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
        'licenses/HYPHENATION-CA.txt': root / 'docs/licenses/HYPHENATION-CA.txt',
        'licenses/HYPHENATION-TR.txt': root / 'docs/licenses/HYPHENATION-TR.txt',
        'licenses/HYPHENATION-HR.txt': root / 'docs/licenses/HYPHENATION-HR.txt',
        'licenses/HYPHENATION-NO.txt': root / 'docs/licenses/HYPHENATION-NO.txt',
        'licenses/HYPHENATION-LT.txt': root / 'docs/licenses/HYPHENATION-LT.txt',
        'licenses/HYPHENATION-IS.txt': root / 'docs/licenses/HYPHENATION-IS.txt',
        'licenses/HYPHENATION-OC.txt': root / 'docs/licenses/HYPHENATION-OC.txt',
        'licenses/HYPHENATION-CY.txt': root / 'docs/licenses/HYPHENATION-CY.txt',
        'licenses/HYPHENATION-PMS.txt': root / 'docs/licenses/HYPHENATION-PMS.txt',
        'licenses/HYPHENATION-RM.txt': root / 'docs/licenses/HYPHENATION-RM.txt',
        'licenses/HYPHENATION-EU.txt': root / 'docs/licenses/HYPHENATION-EU.txt',
        'licenses/HYPHENATION-FUR.txt': root / 'docs/licenses/HYPHENATION-FUR.txt',
        'licenses/HYPHENATION-EO.txt': root / 'docs/licenses/HYPHENATION-EO.txt',
        'licenses/HYPHENATION-KA.txt': root / 'docs/licenses/HYPHENATION-KA.txt',
        'licenses/LPPL-1.3c.txt': root / 'docs/licenses/LPPL-1.3c.txt',
        'licenses/hyph-ru.tex': root / 'assets/hyphenation/hyph-ru.tex',
        'licenses/hyph-sv.tex': root / 'assets/hyphenation/hyph-sv.tex',
        'licenses/hyph-ca.tex': root / 'assets/hyphenation/hyph-ca.tex',
        'licenses/hyph-tr.tex': root / 'assets/hyphenation/hyph-tr.tex',
        'licenses/hyph-hr.tex': root / 'assets/hyphenation/hyph-hr.tex',
        'licenses/hyph-no.tex': root / 'assets/hyphenation/hyph-no.tex',
        'licenses/hyph-lt.tex': root / 'assets/hyphenation/hyph-lt.tex',
        'licenses/hyph-is.tex': root / 'assets/hyphenation/hyph-is.tex',
        'licenses/hyph-oc.tex': root / 'assets/hyphenation/hyph-oc.tex',
        'licenses/hyph-cy.tex': root / 'assets/hyphenation/hyph-cy.tex',
        'licenses/hyph-pms.tex': root / 'assets/hyphenation/hyph-pms.tex',
        'licenses/hyph-rm.tex': root / 'assets/hyphenation/hyph-rm.tex',
        'licenses/hyph-eu.tex': root / 'assets/hyphenation/hyph-eu.tex',
        'licenses/hyph-fur.tex': root / 'assets/hyphenation/hyph-fur.tex',
        'licenses/hyph-ka.tex': root / 'assets/hyphenation/hyph-ka.tex',
        'licenses/eo.pattern': root / 'assets/hyphenation/eo.pattern',
        'licenses/eohyph.m4': root / 'assets/hyphenation/eohyph.m4',
    }
    for file, source in assets.items():
        data = package.read(roots[0] + 'reader/' + file)
        if data != source.read_bytes():
            raise SystemExit('Bundled Reader asset differs: ' + file)
        if file == 'fonts/PrimitiveSymbols.ttf' and sha256(data).hexdigest() != symbol_font_hash:
            raise SystemExit('Nerd Fonts symbol asset differs from its reviewed upstream copy')
print('Verified the pinned SRELL header, binary notices and MIT symbol fallback font.')
