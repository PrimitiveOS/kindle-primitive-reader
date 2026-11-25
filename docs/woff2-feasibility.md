# WOFF 2 font feasibility

WOFF 2 embedded fonts are a reading parity gap. Reader supports WOFF 1 and sfnt publication fonts, but still falls back when a book supplies only WOFF 2. The pinned KOReader Mac runtime renders WOFF 2. No WOFF 2 decoder is enabled, vendored, or packaged in Reader. Physical Kindle execution remains untested.

## Native comparison and fixture

The [authored WOFF 2 EPUB](../tests/fixtures/publisher-woff2.epub) and [TTF control](../tests/fixtures/publisher-woff-ttf.epub) contain the same text and the same Silkscreen face. The WOFF 2 uses transformed `glyf`, `loca`, and `hmtx` tables. In the pinned KOReader Mac runtime, both measured 320 pixels with embedded fonts disabled and 460 pixels when enabled. The replay opens the actual publication, changes the native font setting, finds the same passage, and captures the WOFF 2 page. Hashes of all compared source files remained unchanged. [WOFF 2 on](screenshots/woff2-stock-on.png) · [WOFF 2 off](screenshots/woff2-stock-off.png) · [Native replay](../scripts/koreader-woff-check.lua)

The fixture is generated from Reader's pinned `Silkscreen-Regular.ttf`, whose SIL Open Font License 1.1 notice is included in the EPUB. [Font and notice](../tests/fixtures/fonts/OFL-Silkscreen.txt) · [Hashes and generator versions](../tests/fixtures/publisher-woff2.json). Its generator requires FontTools 4.39.3 and Brotli 1.1.0 only on the development machine. Neither becomes a Reader runtime dependency. FontTools declares MIT; the Python Brotli binding declares MIT. [FontTools license](https://github.com/fonttools/fonttools/blob/4.39.3/LICENSE) · [Brotli license](https://github.com/google/brotli/blob/v1.1.0/LICENSE)

With those versions installed, check the generated EPUB and its manifest:

```sh
python3 scripts/generate-woff2-fixture.py --check
```

To repeat the pinned native comparison in an isolated temporary profile:

```sh
reader="$PWD"
reference="$reader/build/parity-review/koreader-macos-reference"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/reader-woff2.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/cache" "$scratch/profile"
(
  cd "$reference/runtime/koreader"
  env TMPDIR="$scratch" KO_DYLD_PREFIX="$reference/build/staging" \
    KO_HOME="$scratch/profile" SDL_VIDEODRIVER=dummy \
    EMULATE_READER_W=650 EMULATE_READER_H=900 EMULATE_READER_DPI=96 \
    EMULATE_READER_BB_TYPE=BB8 EMULATE_BW_SCREEN=1 \
    ./luajit "$reader/scripts/koreader-woff-check.lua" "$scratch/cache" \
    "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf" "$scratch" \
    "$reader/tests/fixtures/publisher-woff-ttf.epub" \
    "$reader/tests/fixtures/publisher-woff2.epub" woff2
)
```

## Decoder investigation

WOFF 2 uses Brotli and can transform `glyf`, `loca`, and `hmtx` tables, so a Brotli decompressor by itself does not reconstruct a font. The header's `totalSfntSize` is a hint: the W3C specification permits valid reconstructed sizes to differ. A size check based only on that field is insufficient. [W3C header rules](https://www.w3.org/TR/WOFF2/#WOFF2Header) · [WOFF 2 transforms](https://www.w3.org/TR/WOFF2/#table_format)

The pure Rust [Wuff 0.2.10](https://github.com/nicoburns/wuff/tree/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff) decoder handles the transformed tables. Its cached crate had SHA-256 `f6a992b57b3d1ee972b2e3fd9225e44793104d49818836dd812d5459187712dd` and declares upstream revision `0bb77cb3448100b8f560abf796014c8b85d75e23`. It is not in Reader's current lockfile. An isolated offline Mac probe decoded the fixture's 9,048-byte WOFF 2 to a 32,220-byte sfnt. An independent FontTools comparison found matching character maps, glyph order, outlines and advances for all 228 glyphs. The same scratch project cross-compiled offline for `armv7-unknown-linux-musleabihf` into a stripped, statically linked ARM EABI5 executable. That establishes compilation, not ARM execution. No usable ARM Linux emulator was available and the Kindle was not used.

Two scratch configurations compiled: Wuff's optional `brotli-decompressor` 6.3.1, and Wuff with its default features disabled and a callback using Reader's already-locked `brotli` 8.0.4 and `brotli-decompressor` 5.0.3. The first decoded the same face with transformed `glyf` and `loca`; the callback configuration decoded the final three-transform fixture. Both ran on the Mac and cross-compiled offline. The callback limits returned Brotli output, but the probe did not establish a bound on intermediate allocations. Both scratch projects and their outputs were removed after the checks.

## Allocation and licensing gates

Unmodified Wuff is not yet safe to enable under Reader's current 2 MiB per-font limit. It allows up to 128 MiB of decoded data and checks final output after reconstruction. Its directory, glyph, contour, point, metric, and output vectors can allocate separately. Its bundled Brotli allocator also allocates internal workspace independently of returned data. Limiting only the Brotli output or checking the returned sfnt afterward does not establish a bound on peak memory. [Wuff decoder](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/decompress_woff2.rs) · [Glyph reconstruction](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/woff/glyf_decoder.rs) · [Brotli allocator](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/brotli/rust.rs)

Before enabling it, audit a pinned decoder and account for input, decompressed stream, reconstructed output and each intermediate allocation with checked arithmetic and a shared budget. Verify whether the Brotli allocator can fail safely at that budget; if necessary, patch the decoder or isolate the work. Font collections need separate bounds or an explicit unsupported fallback. Add malformed table, oversized window, point-count, transformed `glyf`/`loca`/`hmtx`, truncated stream and allocation-failure tests. Compare rendered glyphs, selection, source positions and fallback against KOReader, then run the resulting binary under ARM Linux before device acceptance. These are outstanding requirements, not completed guarantees.

Wuff 0.2.10 declares MIT. Its upstream [license](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/LICENSE) names the WOFF2 Authors and Nico Burns. The downloaded crate did not contain that license file, so any vendored or distributed use must include the upstream notice. The optional `brotli-decompressor` 6.3.1 declares BSD-3-Clause/MIT and `alloc-no-stdlib` 3.0.0 declares BSD-3-Clause. Reader's existing Brotli 8.0.4, Brotli decompressor 5.0.3, allocation crates and `bytes` 1.12.2 have their current notices in [third-party licenses](licenses/THIRD-PARTY.txt). Any different versions or newly packaged source need their own notices.
