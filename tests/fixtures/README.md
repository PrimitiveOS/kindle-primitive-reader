# Reader verification fixtures

The [manifest](manifest.json) records the expected reading order, locations, links, resources, and errors. Archive paths are relative to the ZIP root. A `#fragment` identifies the XHTML element containing a known passage.

| Fixture | What to check |
| --- | --- |
| `field-notes-epub2.epub` | EPUB 2 NCX navigation, chapter order, publisher CSS, grayscale PNG, local and cross-chapter links, and multilingual text |
| `field-notes-epub3.epub` | EPUB 3 navigation, semantic footnote and back link, embedded font, and the same reading passages as the EPUB 2 book |
| `inline-scripts.xhtml` | Raised footnote links and exponents, lowered formula indices, source navigation and font resizing. |
| `text-decorations.xhtml` | Original proof sheet with a dated correction, underlined instruction, overline and two styled links. |
| `publisher-faces.epub` | Two OFL fonts assigned to four styles and two CSS families. Compare the normal, emphasized and imprint lettering, then restore the source anchor. [Reading checks](../../docs/embedded-font-families.md) |
| `night-landing.epub` | A short and a multi-page endnote in a separate chapter, two inline references, a link back to the passage and an ordinary heading link. Each note has original sample text. |
| `workshop-notes.fb2` | Original FictionBook with an earlier notes section, a later comments section, a backlink, and ordinary paragraph and catalogue links. [Native and simulator comparison](../../docs/fb2-footnotes.md) |
| `harbor-survey.fb3` | FictionBook 3 package with an escaped extensionless body, cover, related picture, nested Contents, table cells, a local note and a backlink. [Native and simulator comparison](../../docs/fb3-reading.md) |
| `harbor-continuation.fb3` | A large section inside a single outer section, with a styled epigraph, inherited language, generated and authored headings, and links in both directions across Reader's split. [Native and simulator comparison](../../docs/fb3-reading.md) |
| `english-hyphenation.epub` | Compare Automatic and Authored modes on identical English text. The language comes from the OPF. Check both fragments of a split word and restore its source anchor. [Simulator comparison](../../docs/english-hyphenation.md) |
| `english-regions.xhtml` | Read the same text with explicit US and British declarations. Compare the different line endings in the [simulated page](../../docs/screenshots/english-regions.png). |
| `long-chapter.epub` | Continuous layout through 240 numbered observations in one chapter. Find observation 137 again after changing the viewport or font size. |
| `rtl-notebook.epub` | A bilingual harbor notebook with explicit right-to-left EPUB progression. Open in two-page mode, select text and follow links on either leaf, then check the single-leaf final chapter. |
| `many-chapters.epub` | Open the first, middle, and last of 48 chapters. Check spine and table of contents order. |
| `missing-image.epub` | A valid ZIP links to a missing image. Check for usable fallback or error handling. |
| `malformed-container.epub` | A valid ZIP contains incomplete package-location XML. Check that opening fails cleanly. |
| `truncated.epub` | The ZIP has no end of central directory record. Check that opening fails cleanly. |
| `beacon-atlas.djvu` | Six original illustrated pages with nested publisher contents, page identifiers, repeated titles, signed displacements, and three inactive headings. [DjVu reference comparison](../../docs/djvu-contents.md) |
| `tilted-scan.pdf` | Two original pages with opposite 8° baselines, selectable words and an internal link. Compare the upright result with the source and check that the link opens the next page. |
| `password-required.pdf` | One generated blank page. Verify the password flow rejects a wrong password and accepts `reader-test`. |
| `field-notes.tcr` | Calibre 9.5.0 TCR output with original Windows-1252 sample text. Verify decompression and non-ASCII punctuation. |

In each of the two valid Field Notes books, `tide-a` and `tide-b` contain the identical sentence `The tide keeps its own time.` Save a location at each occurrence and verify that it resolves correctly after repagination. The chapter also contains Vietnamese, right-to-left Arabic, and Japanese samples. The EPUB 3 `INK SAMPLE` line uses an embedded Silkscreen face. Use available fallback fonts for the other samples, and record their names when comparing glyph coverage.

From the repository root, use Python 3.9 or newer to regenerate or verify the EPUB fixtures and tilted PDF.

```sh
python3 scripts/generate-fixtures.py
python3 scripts/generate-fixtures.py --check
```

The generator uses fixed timestamps and stores ZIP members without compression. It reads the pinned font and notice from this directory and runs offline. Fixture text and artwork are original and use [CC BY-NC-SA 4.0](../../LICENSE-DOCS). Silkscreen Regular retains its [SIL Open Font License 1.1](fonts/OFL-Silkscreen.txt), which is also included in the EPUB 3 archive. [Pinned Google Fonts source](https://github.com/google/fonts/tree/7e8d490955e9d2629cd3095e8dadd6fafe3c16b8/ofl/silkscreen)

[EPUBCheck 5.4.0](https://github.com/w3c/epubcheck/releases/tag/v5.4.0) reported zero errors and warnings for the four valid fixtures. For the three intentionally incomplete fixtures, it reported a missing file (`RSC-001`), invalid container XML (`RSC-016`), and a missing ZIP end header (`PKG-008`), respectively.

EPUBCheck also reported zero errors and warnings for `rtl-notebook.epub`. The Rust system test opens the bottom Layout panel, enables Two pages, follows a link on the left leaf, exits, and continues at the saved location. To reproduce the screenshot, set `READER_RTL_SCREENSHOT` to a PNG path and `READER_TEST_FONTS` to a path list containing Noto Sans and an Arabic fallback. The dark-page image test accepts `READER_NIGHT_IMAGE_SCREENSHOT` and `READER_IMAGE_CONTROLS_SCREENSHOT` and uses Field Notes (EPUB 3). A Japanese fallback also covers the Japanese passage in that fixture.

```sh
cargo test --locked --features kindle --lib rtl_book_opens_from_documents
cargo test --locked --features kindle --lib image_controls_follow_night_mode
```

The password fixture was generated with pypdf 6.1.1, a 120 × 80 point blank page,
and RC4-128 encryption. Its SHA-256 is
`3d2ecdcffc6efdb74e58f660cbf017d3162d42fc08279dc4c274974633b11798`.

The TCR fixture has SHA-256
`a918383808daa49e5cdd101e4f121b6175fcdd2df6ab406b3c33559cb9b67af3`.

The simulated interface uses [Launcher’s Noto Sans Regular](https://github.com/PrimitiveOS/kindle-primitive-launcher/blob/799cd5348d0a82673ea4fc36664ffc084146f238/assets/fonts/NotoSans-Regular.ttf). Its [SIL Open Font License](fonts/OFL-NotoSans.txt) travels with the test bundle.

`fast-pair-kerning.epub` contains an original pair-spacing specimen. Harbor Pair Test is a renamed derivative of Silkscreen Regular with authored legacy AV, VA, and To pair values. It retains the [Silkscreen SIL Open Font License](fonts/OFL-Silkscreen.txt). [Deterministic generator](../../scripts/generate-fast-kerning-fixture.py) · [Fast kerning comparison](../../docs/kerning-modes.md#fast-pair-spacing)

`signal-guide.epub` contains an original two-chapter field notebook with an inline SVG reference and a percentage-sized PNG sketch. The Parley tests verify CSS sizing, image targets, text geometry and source coverage. The Reader test taps the illustrated link, returns, saves and reopens the book. Reproduce the [simulated page](../../docs/screenshots/epub-inline-illustrations.png) using `reader-probe parley tests/fixtures/signal-guide.epub --font <NotoSans-Regular.ttf> --width 1264 --height 1680 --size 44 --margin 72 --output <page.png>`.


`cmyk-print-chart.pdf` contains an original 512 × 384 DeviceCMYK test chart with eight swatches, small tint variations, and a vector registration line. The generator defines every source pixel and PDF object. Reader maps its colors through Hayro’s standard printer-color profile.

## JPEG 2000 pages

OpenJPEG 2.5.4 encoded the original harbor drawings and gradients. Each input
measures 160 × 112 pixels. Use the following files directly in Reader's Library.

| File | Independent comparison |
| --- | --- |
| `harbor-beacon.jp2` | Lossless 8-bit grayscale. Every output pixel matches the original drawing. |
| `signal-flags.j2k` | Four lossless RGB tiles in a raw codestream. Reader's luminance matches the original signal flags pixel for pixel. |
| `evening-light.jp2` | Lossless unsigned 16-bit samples. Every 8-bit display value matches the original intensity scaled to the screen. |
| `harbor-lights.jp2` | Lossy 4:2:0 sYCC. Every 8-bit output luminance is within three levels of the separately decoded `harbor-lights-reference.pgm`. |
| `harbor-signals.cbz` | The beacon and signal flags in page order. Standalone and comic renders match at native size and after a quarter turn. |

Use `python3 scripts/generate-jpeg2000-fixtures.py` to compare the committed
files with OpenJPEG. The optional `--write` argument regenerates them. Only the
fixture generator uses the OpenJPEG command-line programs. Reader uses the Rust
decoder on the host and Kindle. Run `cargo test --locked --features kindle --lib jpeg2000`
to check the pixel comparisons and normal Library workflow. Set
`READER_JPEG2000_SCREENSHOT` to a PNG path to capture the simulated beacon
page. [OpenJPEG](https://www.openjpeg.org/) · [Reader decoder tests](../../src/jpeg2000.rs)


[publisher-lists.xhtml](publisher-lists.xhtml) is an original short publication with a numbered checklist, nested bullets, a reversed alphabetical list, a source-authored ordinal and a list with its markers disabled in CSS. Its text and artwork may be redistributed under the project licence.

`paragraph-indents.xhtml` is an original short coastal journal for comparing inherited first-line indents, an explicit zero, a percentage of the reading column and hanging lines. Its text and stylesheet were written for Reader.

`paragraph-opener.xhtml` is an original harbour journal that checks publisher styling for a paragraph after a heading and the first paragraph of a section.

`nearby-search.xhtml` is an original short harbour log with a repeated sentence for directional search and return-to-location checks.

`gif-signals.epub` contains an original semaphore diagram, a transparent GIF in inline and packaged SVG, a GIF cover, a linked full-size illustration and a two-frame GIF. Pillow 12.1 generates the fixture and independently supplies full first-frame pixel digests for comparison. [Fixture generator](../../scripts/generate-gif-epub.py) · [Reference generator](../../scripts/epub-gif-reference.py)


`harbour-cjk-width.epub` contains an original horizontal Japanese type specimen, fullwidth and halfwidth forms, Hangul, punctuation, and a Latin width control. The embedded *Harbour Test Gothic* is a renamed subset of Noto Sans CJK JP. FontTools retained the original outlines, UTF-8 mappings and advances for the sample characters. The source font's SIL Open Font License travels inside the EPUB at `META-INF/font-license.txt`. [Original font](https://github.com/notofonts/noto-cjk/blob/523d033d6cb47f4a80c58a35753646f5c3608a78/Sans/OTF/Japanese/NotoSansCJKjp-Regular.otf) · [SIL Open Font License](https://github.com/notofonts/noto-cjk/blob/523d033d6cb47f4a80c58a35753646f5c3608a78/LICENSE)

`harbor-vertical-viewport.epub` has original Japanese and Traditional Chinese vertical passages. Each chapter repeats a linked word at two source offsets and links to the other chapter. The last chapter has a ruby group that fits the test's portrait column and exceeds its landscape height. The 37 KB renamed Noto Sans CJK JP subset retains the font's vertical substitutions and includes its SIL Open Font License in `META-INF/font-license.txt`. [Regeneration script](../../scripts/generate-vertical-viewport.py) · [Viewport and native comparison](../../docs/reading-edition-orientation.md)
