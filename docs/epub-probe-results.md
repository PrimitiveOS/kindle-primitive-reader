# EPUB feasibility results

**Use Parley with direct grayscale glyph rendering for Reader's EPUB
engine.** Blitz renders a broader set of publisher styles and provides a
reference for layout comparisons. Its uncached raster time on the Oasis 3
exceeded the proposed budget. The Parley prototype keeps the shared EPUB
positions and supports the heading styles, paragraph background, embedded
font, and bitmap illustration in the Field Notes fixture.

## Oasis 3 software measurements

The device ran firmware 5.18.2. Both probes used a 1264 × 1606 reading viewport,
42-pixel Noto Sans, 54-pixel margins, and a 1.35 default line-height multiplier.
Each row summarizes five fresh processes and 30 additional rasterizations per
process. The p95 column is the median of the five per-process p95 results.
RSS is the median of each process's high-water mark.

| Fixture | Engine and renderer | First frame | Warm p95 | Peak RSS |
|---|---|---:|---:|---:|
| Field Notes | KOReader CREngine 2026.07.1 | 222 ms | 10 ms | 7.4 MiB |
| Field Notes | Blitz with Vello | 373 ms | 240 ms | 17.8 MiB |
| Field Notes | Parley with Vello | 201 ms | 161 ms | 12.2 MiB |
| Field Notes | Parley with direct grayscale | 65 ms | 9.1 ms | 6.1 MiB |
| Long Chapter | KOReader CREngine 2026.07.1 | 267 ms | 13 ms | 7.3 MiB |
| Long Chapter | Blitz with Vello | 707 ms | 441 ms | 21.5 MiB |
| Long Chapter | Parley with Vello | 542 ms | 265 ms | 17.3 MiB |
| Long Chapter | Parley with direct grayscale | 276 ms | 13.5 ms | 9.4 MiB |

CREngine laid out the entire book through the installed KOReader libraries,
with an empty engine cache for each process. Reader laid out the selected
chapter. Page counts vary with engine spacing and pagination. Each run used
the ordinary filesystem cache state. These measurements end when a frame is
ready in memory. They exclude the physical E Ink refresh, and single-page
chapters can include redraws of the same page in the warm sequence.

Direct grayscale and tuning reduced Parley's long-chapter p95 from 265 to
13.5 ms. Both fixtures met the proposed first-frame budget of 1.5× and warm
p95 budget of 1.25× against CREngine. An experiment that varied the Blitz
raster height by page made its warm performance less consistent on Kindle.
The comparison implementation uses the fixed-height viewport again.

## Desktop and interactive verification

- The reproducible corpus includes valid EPUB 2 and 3, long and many-chapter
  books, a missing-image case, malformed XML, and a truncated ZIP. EPUBCheck
  passed the four valid fixtures and reported the expected issue in each
  negative fixture. The seven Rust tests passed, including the rapid
  page-input pacing test.
- Both engines produced grayscale pages and mapped all extracted source
  text in the tested chapters. Saved positions distinguished both repeats
  of a quote after font, margin, line spacing, and viewport changes.
  Tests resolved and laid out the fixture's footnote and its backlink.
  With the named fallback fonts, the Vietnamese, Arabic, and Japanese
  fixture rendered with zero missing glyphs on desktop and in the Parley
  Kindle render. The Kindle probe restored the same passage after a
  fresh process changed the viewport and font. Both engines also rendered
  chapter I of the Standard Ebooks edition of *The Time Machine* assembled
  from its source locally.
  [Fixture provenance](../tests/fixtures/README.md) ·
  [The Time Machine source](https://github.com/standardebooks/h-g-wells_the-time-machine)
- The owner opened the Blitz display probe from Launcher and verified
  forward and back physical page turns, return to the same page after
  sleep and wake, and **Done** returning to a responsive Launcher.
  Its logged first frame was ready in 605 ms. Cached turns had a median
  software time of 0.89 ms across 19 samples. The 22 cache misses had a
  median of 372 ms. Those timings end before drawing the Launcher canvas
  and updating the screen.
- The owner also confirmed legible Parley text, both physical page buttons,
  and return to Launcher. Rapid repeated presses caused a flash over the
  right side. The log showed successive partial AUTO submissions for a large
  changed rectangle. The Reader probe now applies queued directions in order
  and submits at most one page refresh per 450 ms during a burst. Ordinary
  page presses can draw immediately after that interval. The owner repeated
  the fast forward and back presses and confirmed the flash was gone and all
  presses counted. The device log recorded batches of up to three presses.
- In the first USB export and eject check, Reader returned to the same
  page and the page button worked again. The probe showed a text screen
  during export. The shared runtime now uses Launcher's embedded
  `assets/usb-drive-mode.png`. The desktop comparison confirmed that both
  render paths produced the same pixels. The owner verified the correct
  artwork on the Oasis 3 and Reader returned to the same page after eject.
  Reader pins the published Tools fix.
  [Shared USB artwork fix](https://github.com/PrimitiveOS/kindle-primtitive-tools/commit/398679696d19e0334a164775a3740cc700ffc9de)
- Rust 1.100.0 and Zig 0.17.1 produced static ARMHF executables. The
  locked build also succeeded in a separate workspace using a clean
  PrimitiveOS Tools checkout at revision `9f7e331`. The project licences
  and bundled Silkscreen OFL notice are in the repo. Before a Store release,
  review and package the precise dependency notices, including the
  MPL-2.0 CSS components and the selected dual-licence terms for Tools.

## Remaining P0 validation

Verify interactive footnote and return navigation and physical display timing
against KOReader.
Expand coverage of longer books and publisher styles within the declared
EPUB subset. The [execution plan](execution-plan.md) defines the gates
for the daily-reader build and publication.

## Expanded desktop corpus

Parley laid out and rasterized the first page of all 12 narrative chapters of
the Standard Ebooks illustrated edition of *Alice’s Adventures in Wonderland*.
The run covered all 115,703 extracted source characters and reported zero
missing glyphs using the Launcher’s Noto Sans regular and bold fonts. On the
Mac, first-frame software times ranged from 7.7 to 17.7 ms. A separate 30-frame
run in chapter 1 measured a warm rasterization p95 of 0.47 ms. These measurements
cover Mac rendering. Kindle performance and publisher layout fidelity have
separate acceptance checks. [Illustrated EPUB source](https://standardebooks.org/ebooks/lewis-carroll/alices-adventures-in-wonderland/john-tenniel)

The tested EPUB archive has SHA-256
`e8a352212579562c9b0ab6c2439c635886409d75812288547da15f3d18c2641d`.
