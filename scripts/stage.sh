#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

case "${1:-armhf}" in
    armhf|armv7-unknown-linux-musleabihf) target=armv7-unknown-linux-musleabihf ;;
    *) echo "Unsupported package architecture: $1" >&2; exit 1 ;;
esac

stage=${2:-build/stage}
binary=${CARGO_TARGET_DIR:-target}/$target/release/reader-kindle
ocr=${READER_OCR_BUILD_DIR:-}
[ -x "$binary" ] || { echo "Build $binary before staging." >&2; exit 1; }
set -- "$binary"
if [ -n "$ocr" ]; then
    ocr_engine=$ocr/build-tesseract-linux41/bin/tesseract
    ocr_model=$ocr/tessdata/eng.traineddata
    [ -x "$ocr_engine" ] && [ -r "$ocr_model" ] || { echo "Build the OCR engine and model before staging." >&2; exit 1; }
    set -- "$@" "$ocr_engine"
fi
python3 - "$@" <<'PY'
import sys

for name in sys.argv[1:]:
    with open(name, "rb") as binary:
        header = binary.read(52)
    if (
        len(header) != 52
        or header[:7] != b"\x7fELF\x01\x01\x01"
        or header[18:20] != b"\x28\x00"
        or int.from_bytes(header[36:40], "little") & 0x600 != 0x400
    ):
        sys.exit(f"Expected a 32-bit hard-float ARM ELF binary at {name}.")
PY

install -d "$stage/reader/bin" "$stage/reader/licenses" "$stage/documents"
install -m 755 "$binary" "$stage/reader/bin/reader-kindle"
install -m 755 deploy/Reader.sh "$stage/documents/Reader.sh"
install -m 644 docs/licenses/README.md docs/licenses/THIRD-PARTY.txt "$stage/reader/licenses/"
cp -R docs/licenses/pdf docs/licenses/runtime "$stage/reader/licenses/"
if [ -n "$ocr" ]; then
    install -d "$stage/reader/ocr/tessdata" "$stage/reader/licenses/ocr"
    install -m 755 "$ocr_engine" "$stage/reader/ocr/tesseract"
    install -m 644 "$ocr_model" "$stage/reader/ocr/tessdata/eng.traineddata"
    install -m 644 docs/licenses/ocr/* "$stage/reader/licenses/ocr/"
fi
