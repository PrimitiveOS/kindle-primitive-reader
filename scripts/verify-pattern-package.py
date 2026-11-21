#!/usr/bin/env python3
"""Verify the pinned expression engine and redistribution notices in a staged package ZIP."""
from hashlib import sha256
from pathlib import Path
import argparse
from zipfile import ZipFile

root = Path(__file__).resolve().parents[1]
header_hash = '80c2d6436d76822ef8722a7132845e6d9e0231bc4f90b57e5999a608b9aa9101'
notice_hash = '013ba4e9f5ba60c72efa40f638d09b3d1a72c9a4787560207179710e1060d3be'
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
print('Verified the pinned SRELL header and four binary distribution notices.')
