#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
recipe=$PWD/scripts
work=${1:-build/ocr-armhf}
mkdir -p "$work"
cd "$work"
work=$PWD
[ "$(zig version)" = 0.17.1 ] || { echo "Use Zig 0.17.1 for the pinned OCR runtime." >&2; exit 1; }
mkdir -p downloads source bin tessdata

fetch() {
    path=$1
    url=$2
    expected=$3
    if [ ! -f "$path" ]; then
        curl --fail --silent --show-error --location --retry 2 "$url" --output "$path.part"
        mv "$path.part" "$path"
    fi
    python3 - "$path" "$expected" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
with path.open("rb") as source:
    for block in iter(lambda: source.read(1024 * 1024), b""):
        digest.update(block)
if digest.hexdigest() != sys.argv[2]:
    raise SystemExit(f"SHA-256 mismatch for {path}")
PY
}
fetch downloads/leptonica-1.87.0.tar.gz \
    https://codeload.github.com/DanBloomberg/leptonica/tar.gz/13275a278eb55b5746e33f95fbf5a2c8f604b3ab \
    e72242133adf678e8584081a506ca3a665c8f6f6de8c3fe7b47d1d5fcf9a4ce4
fetch downloads/tesseract-5.5.4.tar.gz \
    https://codeload.github.com/tesseract-ocr/tesseract/tar.gz/7e548e6eff172f38f97664afe7f7ce40db36c8d6 \
    445603ab1d0a63df552e193a600d9d5c4021aff36e144f3091af149e5efb381a
fetch tessdata/eng.traineddata \
    https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/65727574dfcd264acbb0c3e07860e4e9e9b22185/eng.traineddata \
    7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2

for component in leptonica tesseract; do
    if [ ! -f "source/$component/CMakeLists.txt" ]; then
        mkdir -p "source/$component"
        tar -xzf downloads/"$component"-*.tar.gz --strip-components=1 -C "source/$component"
    fi
done
cp "$recipe/ocr-armhf.cmake" armhf-linux41.cmake

cat > bin/zig41-cc <<'TOOL'
#!/bin/sh
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec zig cc -target arm-linux.4.1.15-musleabihf -mcpu=cortex_a7 "-ffile-prefix-map=$root=/usr/src/primitiveos-ocr" "$@"
TOOL
cat > bin/zig41-c++ <<'TOOL'
#!/bin/sh
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec zig c++ -target arm-linux.4.1.15-musleabihf -mcpu=cortex_a7 "-ffile-prefix-map=$root=/usr/src/primitiveos-ocr" "$@"
TOOL
for tool in ar ranlib; do
    printf '#!/bin/sh\nexec zig %s "$@"\n' "$tool" > "bin/zig-$tool"
done
chmod 755 bin/zig41-cc bin/zig41-c++ bin/zig-ar bin/zig-ranlib

export ZIG_GLOBAL_CACHE_DIR="$work/zig-global"
export ZIG_LOCAL_CACHE_DIR="$work/zig-local"
export SOURCE_DATE_EPOCH=1793145600
jobs=${READER_OCR_JOBS:-8}
cmake -S source/leptonica -B build-leptonica-linux41 -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE:FILEPATH="$work/armhf-linux41.cmake" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX:PATH="$work/install-linux41" \
    -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=OFF -DBUILD_PROG=OFF \
    -DENABLE_ZLIB=OFF -DENABLE_PNG=OFF -DENABLE_GIF=OFF -DENABLE_JPEG=OFF \
    -DENABLE_TIFF=OFF -DENABLE_WEBP=OFF -DENABLE_OPENJPEG=OFF
cmake --build build-leptonica-linux41 --parallel "$jobs"
cmake --install build-leptonica-linux41

cmake -S source/tesseract -B build-tesseract-linux41 -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE:FILEPATH="$work/armhf-linux41.cmake" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX:PATH="$work/install-linux41" \
    -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_PREFIX_PATH:PATH="$work/install-linux41" \
    -DLeptonica_DIR:PATH="$work/install-linux41/lib/cmake/leptonica" \
    -DBUILD_SHARED_LIBS=OFF -DBUILD_TRAINING_TOOLS=OFF -DBUILD_TESTS=OFF \
    -DINSTALL_CONFIGS=OFF -DOPENMP_BUILD=OFF -DGRAPHICS_DISABLED=ON \
    -DDISABLED_LEGACY_ENGINE=ON -DFAST_FLOAT=ON -DENABLE_NATIVE=OFF -DENABLE_LTO=OFF \
    -DDISABLE_ARCHIVE=ON -DDISABLE_CURL=ON -DDISABLE_TIFF=ON -DLEPT_TIFF_RESULT=1 \
    -DENABLE_PRECOMPILED_HEADERS=OFF -DENABLE_CCACHE=OFF -DENABLE_NINJA_POOL=OFF
cmake --build build-tesseract-linux41 --target tesseract --parallel "$jobs"
file build-tesseract-linux41/bin/tesseract
