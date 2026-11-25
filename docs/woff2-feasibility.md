# WOFF 2 font feasibility

A candidate isolated decoder has been implemented for WOFF 2 embedded fonts.
The pinned KOReader Mac runtime renders WOFF 2. The candidate uses a separate
worker to contain malformed publisher fonts; its resource bounds and behavior
still require testing. Simulator checks and physical Kindle execution remain pending.

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

The pure Rust [Wuff 0.2.10](https://github.com/nicoburns/wuff/tree/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff) decoder handles the transformed tables. Its cached crate had SHA-256 `f6a992b57b3d1ee972b2e3fd9225e44793104d49818836dd812d5459187712dd` and declares upstream revision `0bb77cb3448100b8f560abf796014c8b85d75e23`. It is pinned in Reader's lockfile. An isolated offline Mac probe decoded the fixture's 9,048-byte WOFF 2 to a 32,220-byte sfnt. An independent FontTools comparison found matching character maps, glyph order, outlines and advances for all 228 glyphs. The same scratch project cross-compiled offline for `armv7-unknown-linux-musleabihf` into a stripped, statically linked ARM EABI5 executable. That establishes compilation, not ARM execution. No usable ARM Linux emulator was available and the Kindle was not used.

Two scratch configurations compiled: Wuff's optional `brotli-decompressor` 6.3.1, and Wuff with its default features disabled and a callback using Reader's already-locked `brotli` 8.0.4 and `brotli-decompressor` 5.0.3. The first decoded the same face with transformed `glyf` and `loca`; the callback configuration decoded the final three-transform fixture. Both ran on the Mac and cross-compiled offline. The callback limits returned Brotli output, but the probe did not establish a bound on intermediate allocations. Both scratch projects and their outputs were removed after the checks.

## Isolated decoder candidate

On Linux, `reader-woff2-worker` installs a 64 MiB address-space limit,
two-second soft and three-second hard CPU limits, and a zero core-file limit
before reading font data. It fails closed if those limits cannot be installed.
Input and reconstructed sfnt output are each capped at 2 MiB, and the parent
validates the output before registering it. The address-space limit is a
process ceiling, not a claim that peak decode memory is 2 MiB.

The parent drains both pipes concurrently and applies a shared 1.5-second
wall-clock budget to all WOFF 2 faces in a chapter. It kills and reaps timed
out helpers and falls through to the next CSS `src` when decoding is absent,
busy, invalid or over budget. A bounded in-memory cache avoids repeatedly
starting a worker for identical content. Actual Oasis startup time, page-turn
latency, memory pressure, sleep and USB behavior are not yet measured.

The Mac host does not provide an enforceable Linux address-space limit in
this environment. Debug-only simulation can invoke the helper with a strict
SHA-256 allowlist for the authored WOFF 2 fixture. Other host fonts fall back.
The allowlist is not enabled in release builds and does not alter the Kindle
path. [Worker](../src/bin/woff2-worker.rs) · [Parent protocol](../src/woff2.rs)

## Allocation and licensing gates

Unmodified Wuff is not yet safe to enable under Reader's current 2 MiB per-font limit. It allows up to 128 MiB of decoded data and checks final output after reconstruction. Its directory, glyph, contour, point, metric, and output vectors can allocate separately. Its bundled Brotli allocator also allocates internal workspace independently of returned data. Limiting only the Brotli output or checking the returned sfnt afterward does not establish a bound on peak memory. [Wuff decoder](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/decompress_woff2.rs) · [Glyph reconstruction](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/woff/glyf_decoder.rs) · [Brotli allocator](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/wuff/src/brotli/rust.rs)

The candidate uses OS process limits rather than assuming every internal
allocation in Wuff is fallible. Malformed input, CPU exhaustion and allocation
failure must stay confined to the helper, and absence or failure must preserve
the next publisher fallback. An ARM Linux execution test and real Kindle
acceptance remain outstanding. Font collections and malformed transformation
cases still need wider coverage; no exact compatibility claim is made for them.

Wuff 0.2.10 declares MIT. Its upstream [license](https://github.com/nicoburns/wuff/blob/0bb77cb3448100b8f560abf796014c8b85d75e23/LICENSE) names the WOFF2 Authors and Nico Burns. The downloaded crate did not contain that license file, so any vendored or distributed use must include the upstream notice. The optional `brotli-decompressor` 6.3.1 declares BSD-3-Clause/MIT and `alloc-no-stdlib` 3.0.0 declares BSD-3-Clause. Reader's existing Brotli 8.0.4, Brotli decompressor 5.0.3, allocation crates and `bytes` 1.12.2 have their current notices in [third-party licenses](licenses/THIRD-PARTY.txt). Any different versions or newly packaged source need their own notices.
