# EPUB feasibility probe

The probe opens one chapter of a local EPUB and lays it out with either the
offline Blitz engine or the compact Parley engine. Both use the same archive
loader, source text positions, grayscale output, and benchmark command.
See the [execution plan](execution-plan.md) for the P0 acceptance checks.

## Desktop comparison

Run the fixture checks and build the executable from the Reader repo.

```sh
python3 scripts/generate-fixtures.py --check
cargo test --locked --release
cargo build --locked --release --bin reader-probe
```

Use the same font files and settings for each candidate. Choose a body font
that covers the Latin fixture text and supply additional Arabic and CJK fonts
for the multilingual visual check. This example uses the local Browser
checkout's Noto Serif font. The probe accepts additional `--font` paths in order
of preference.

```sh
mkdir -p build/p0
for engine in blitz parley; do
    target/release/reader-probe "$engine" tests/fixtures/field-notes-epub3.epub \
        --font ../doannt-kindle-browser/assets/fonts/NotoSerif-Regular.ttf \
        --height 1606 --fragment tide-b --anchor-out "build/p0/$engine-position.json" \
        --output "build/p0/$engine-epub3.png"
    target/release/reader-probe "$engine" tests/fixtures/field-notes-epub3.epub \
        --font ../doannt-kindle-browser/assets/fonts/NotoSerif-Regular.ttf \
        --width 1606 --height 1264 --size 56 --margin 72 --line-height 1.5 \
        --anchor-in "build/p0/$engine-position.json"
    target/release/reader-probe "$engine" tests/fixtures/long-chapter.epub \
        --font ../doannt-kindle-browser/assets/fonts/NotoSerif-Regular.ttf \
        --height 1606 --turns 30
done
```

Each run prints one JSON record with layout time, time to the first raster,
page count, source character coverage, missing glyphs, and warm rasterization
statistics.
On Linux it also reports the process high-water RSS. Use `/usr/bin/time -l`
for peak RSS on macOS. Repeat runs in fresh processes for a cold-open sample.
Compare raster time separately from physical E Ink refresh time. Inspect the
captures for glyph fallback, chapter continuity, styles, and illustrations.
Source character coverage measures the extracted text ranges represented by
layout lines.

## Kindle comparison

```sh
cargo zigbuild --locked --release --features kindle --target armv7-unknown-linux-musleabihf --bins
```

`reader-probe` performs the same headless measurement on-device.
`reader-kindle` displays the grayscale pages through PrimitiveOS Tools.
Its viewport is 1264 × 1606 with a 74-pixel footer. Pass the engine name,
book path, and one or more font paths when launching the display probe from
the existing Launcher supervisor. The physical buttons, horizontal swipes,
and taps at either edge turn pages. The probe prepares up to two adjacent
pages during idle time. **Done** exits back through the supervisor.

The probe's logged frame-ready time is a software measurement. Use an
independent screen observation for time to a visible page. Compare KOReader
on the same device and matched corpus, viewport and font settings. Record
the toolchain, device firmware, candidate revision, and baseline version
with the results.

## Current probe scope

- The first parser uses `zip` and `roxmltree` with explicit container and
  resource limits. `rbook` 0.7.13 was evaluated for package reading. Its ZIP
  adapter copies resource data through a generic writer. This probe keeps
  the limits for archive entries, XML and chapter loads in one adapter.
  [rbook ZIP adapter](https://docs.rs/rbook/0.7.13/src/rbook/ebook/archive/zip.rs.html)
- Blitz applies XHTML and CSS, with an archive-only loader for local CSS,
  PNG, JPEG, and TrueType or OpenType font resources. Its pagination maps
  rendered line ranges to source blocks when the layout text matches.
- Parley lays out structural paragraphs and paints bounded grayscale glyph
  masks directly. It supports simple publisher selectors, embedded TTF or OTF
  fonts, grayscale paragraph backgrounds, and PNG or JPEG illustrations.
  Full CSS layout and exact inline image positioning remain separate capabilities.
- A saved position identifies the package revision, chapter revision, and
  text block with a UTF-8 offset. Cross-edition recovery, the consumer-facing
  library, and annotation UI are later work.
- The file reader supports a bounded single-volume ZIP32 archive and UTF-8
  reflowable XHTML. The prototype rejects mixed structure with text between
  nested blocks and uses the declared subset for compatibility claims.
