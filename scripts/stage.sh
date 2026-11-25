#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

case "${1:-armhf}" in
    armhf|armv7-unknown-linux-musleabihf) target=armv7-unknown-linux-musleabihf ;;
    *) echo "Unsupported package architecture: $1" >&2; exit 1 ;;
esac

stage=${2:-build/stage}
binary=${CARGO_TARGET_DIR:-target}/$target/release/reader-kindle
woff2_worker=${CARGO_TARGET_DIR:-target}/$target/release/reader-woff2-worker
ocr=${READER_OCR_BUILD_DIR:-build/ocr-armhf}
[ -x "$binary" ] || { echo "Build $binary before staging." >&2; exit 1; }
[ -x "$woff2_worker" ] || { echo "Build $woff2_worker before staging." >&2; exit 1; }
[ -r assets/reader-icon.png ] || { echo "Missing the Reader icon." >&2; exit 1; }
[ -r assets/reader-icon-dark.png ] || { echo "Missing the Reader dark icon." >&2; exit 1; }
[ -r assets/fonts/PrimitiveSymbols.ttf ] || { echo "Missing the symbol fallback font." >&2; exit 1; }
icon="# Icon: data:image/png;base64,$(base64 < assets/reader-icon.png | tr -d '\n')"
icon_dark="# IconDark: data:image/png;base64,$(base64 < assets/reader-icon-dark.png | tr -d '\n')"
set -- "$binary" "$woff2_worker"
if [ -z "${READER_OCR_BUILD_DIR:-}" ]; then
    sh scripts/build-ocr-armhf.sh "$ocr"
fi
ocr_engine=$ocr/build-tesseract-linux41/bin/tesseract
ocr_model=$ocr/tessdata/eng.traineddata
ocr_vietnamese_model=$ocr/tessdata/vie.traineddata
[ -x "$ocr_engine" ] && [ -r "$ocr_model" ] && [ -r "$ocr_vietnamese_model" ] || { echo "Build the OCR engine and English and Vietnamese models before staging." >&2; exit 1; }
set -- "$@" "$ocr_engine"
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

install -d "$stage/reader/bin" "$stage/reader/fonts" "$stage/reader/licenses" "$stage/documents"
install -m 755 "$binary" "$stage/reader/bin/reader-kindle"
install -m 755 "$woff2_worker" "$stage/reader/bin/reader-woff2-worker"
install -m 644 assets/fonts/PrimitiveSymbols.ttf "$stage/reader/fonts/PrimitiveSymbols.ttf"
awk -v icon="$icon" -v icon_dark="$icon_dark" '{ print } /^# Author:/ { print icon; print icon_dark }' deploy/Reader.sh > "$stage/documents/Reader.sh"
chmod 755 "$stage/documents/Reader.sh"
install -m 644 LICENSE LICENSE-DOCS "$stage/reader/licenses/"
install -m 644 docs/licenses/README.md docs/licenses/THIRD-PARTY.txt docs/licenses/ENGLISH-HYPHENATION.txt \
    docs/licenses/HYPHENATION-ES-IT-PT.txt docs/licenses/HYPHENATION-ET-FI.txt \
    docs/licenses/HYPHENATION-DA-NL.txt docs/licenses/HYPHENATION-FR-DE.txt \
    docs/licenses/HYPHENATION-BG-GL-GA.txt docs/licenses/HYPHENATION-PL.txt \
    docs/licenses/HYPHENATION-RU.txt docs/licenses/HYPHENATION-SV.txt docs/licenses/HYPHENATION-CA.txt docs/licenses/HYPHENATION-TR.txt docs/licenses/HYPHENATION-HR.txt docs/licenses/HYPHENATION-NO.txt docs/licenses/HYPHENATION-LT.txt docs/licenses/HYPHENATION-IS.txt docs/licenses/HYPHENATION-OC.txt docs/licenses/HYPHENATION-CY.txt docs/licenses/HYPHENATION-PMS.txt docs/licenses/HYPHENATION-RM.txt docs/licenses/HYPHENATION-EU.txt docs/licenses/HYPHENATION-FUR.txt docs/licenses/HYPHENATION-EO.txt docs/licenses/HYPHENATION-KA.txt docs/licenses/HYPHENATION-LA.txt docs/licenses/HYPHENATION-EL.txt docs/licenses/HYPHENATION-SK-SL.txt docs/licenses/LPPL-1.3c.txt docs/licenses/WUFF.txt \
    docs/licenses/SRELL.txt docs/licenses/NERDFONTS.txt "$stage/reader/licenses/"
cp -R docs/licenses/pdf docs/licenses/runtime "$stage/reader/licenses/"
# Preserve upstream pattern sources and their notices with the compiled tables.
install -m 644 assets/hyphenation/hyph-ru.tex assets/hyphenation/hyph-sv.tex assets/hyphenation/hyph-ca.tex assets/hyphenation/hyph-tr.tex assets/hyphenation/hyph-hr.tex assets/hyphenation/hyph-no.tex assets/hyphenation/hyph-lt.tex assets/hyphenation/hyph-is.tex assets/hyphenation/hyph-oc.tex assets/hyphenation/hyph-cy.tex assets/hyphenation/hyph-pms.tex assets/hyphenation/hyph-rm.tex assets/hyphenation/hyph-eu.tex assets/hyphenation/hyph-fur.tex assets/hyphenation/hyph-ka.tex assets/hyphenation/hyph-la-x-classic.tex assets/hyphenation/hyph-la-x-liturgic.tex assets/hyphenation/hyph-grc.tex assets/hyphenation/eo.pattern assets/hyphenation/eohyph.m4 "$stage/reader/licenses/"
install -d "$stage/reader/ocr/tessdata" "$stage/reader/licenses/ocr"
install -m 755 "$ocr_engine" "$stage/reader/ocr/tesseract"
install -m 644 "$ocr_model" "$stage/reader/ocr/tessdata/eng.traineddata"
install -m 644 "$ocr_vietnamese_model" "$stage/reader/ocr/tessdata/vie.traineddata"
install -m 644 docs/licenses/ocr/* "$stage/reader/licenses/ocr/"
