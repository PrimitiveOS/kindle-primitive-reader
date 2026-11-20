#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

case "${1:-armhf}" in
    armhf|armv7-unknown-linux-musleabihf) target=armv7-unknown-linux-musleabihf ;;
    *) echo "Unsupported package architecture: $1" >&2; exit 1 ;;
esac

stage=${2:-build/stage}
binary=${CARGO_TARGET_DIR:-target}/$target/release/reader-kindle
[ -x "$binary" ] || { echo "Build $binary before staging." >&2; exit 1; }
python3 - "$binary" <<'PY'
import sys

with open(sys.argv[1], "rb") as binary:
    header = binary.read(52)
if (
    len(header) != 52
    or header[:7] != b"\x7fELF\x01\x01\x01"
    or header[18:20] != b"\x28\x00"
    or int.from_bytes(header[36:40], "little") & 0x600 != 0x400
):
    sys.exit("Expected a 32-bit hard-float ARM ELF binary.")
PY

install -d "$stage/reader/bin" "$stage/reader/licenses" "$stage/documents"
install -m 755 "$binary" "$stage/reader/bin/reader-kindle"
install -m 755 deploy/Reader.sh "$stage/documents/Reader.sh"
install -m 644 docs/licenses/README.md docs/licenses/THIRD-PARTY.txt "$stage/reader/licenses/"
cp -R docs/licenses/pdf docs/licenses/runtime "$stage/reader/licenses/"
