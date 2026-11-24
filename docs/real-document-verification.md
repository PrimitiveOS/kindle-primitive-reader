# Opening books from Kindle Documents

The compatibility run uses the owner's existing books on an Oasis 3 with firmware 5.18.2.
The reference is KOReader v2026.07.1, the version installed on this device.
Public records include aggregate outcomes. The original files, names, book text,
screenshots of personal books and saved reading positions are absent from the repository.

## What the test exercises

The device diagnostic runs the Rust Reader code natively on the Oasis and paints
to an off-screen canvas. It browses folders under Documents through the normal
picker, dispatches the same page-button input used to reach later rows, selects
the file, renders the first page and sends a forward input event. It writes its
own scratch profile, restarts and opens Continue reading to compare the saved
position and rendered page body. This software path can run while another app is
in the foreground. It does not generate physical touch or page-key events;
foreground touch, E Ink timing and sleep are separate checks.

## Reader 0.1.23 release on the Oasis

On 22 November 2026, Store staging verified the published ARMHF ZIP and its
SHA-256 (`49562aae41bab0d5e6e635fa284d71e536e7155cd77c42568e45f5a90130d8b8`).
The package was installed on the owner's awake Oasis 3 through Store's verified
install transaction. The installed record now reports version 0.1.23. The normal
picker opened one real EPUB and one real PDF from Documents. Their names and contents
are intentionally omitted. The release binary's SHA-256 is
`5c4b23986cb95dcedb1cc9578062986395731b6cf535017195bed9674ca9fb16`.

| Check | EPUB | PDF |
| --- | ---: | ---: |
| First frame | 2,087 ms; visible content | 759 ms; visible content |
| Forward page event | Changed the displayed page | Changed the displayed page |
| Continue reading after restart | Passed | Passed |
| Sampled spine | 3 of 24 chapters; 31 pages painted | Not applicable |
| Source coverage | 24,416 characters mapped; 7/7 page anchors; 1/1 image decoded | Not applicable |
| Reported issues / missing glyphs | 0 / 0 | No reported error / not measured |

Both diagnostic runs exited successfully. Their scratch profiles and temporary
installer files were removed afterward; Reader's data folder was preserved. This
verifies the published executable's picker, parsing, layout, rendering and saved-
position paths on the actual device. It does not claim a physical touch or e-ink
timing test.

## Current worktree candidate on the Oasis

On 23 November 2026, the awake Oasis ran the current static ARMHF development
binary from a temporary test directory; the installed Reader package and user
data were not replaced. The binary SHA-256 is
`d28379bb12cffe9b7906e195465b7c890df4535a3c4bfdd1d1ff11d39c536322`. Its
native test bundle passed 921 Reader tests, 14 EPUB package tests and four layout
probes; 62 Reader tests were ignored because they require private or
device-provided inputs.

The candidate's normal picker and resume diagnostic visited 183 filesystem
entries using a new scratch profile per entry. All 134 selectable supported
documents passed open and resume: 31 EPUB, 16 AZW3, two MOBI, 20 PDF, four TXT,
37 JPEG, one PNG and 23 XHTML files. The diagnostic sent a forward input and
compared the saved reading frame after restarting into Continue reading. Three
protected AZW files and ten KFX files returned Kindle Library guidance. Thirty-
six image paths inside sidecar or packaged-book folders were not picker entries
and were classified as not selectable. One PDF had a transient first-run open
failure; two independent fresh-profile reruns passed. No personal filenames,
page text, or screenshots were retained.

This verifies the development binary's ARM software picker, parsing, rendering
and saved-position paths against the owner's current Documents collection. It
does not cover foreground touch, physical page keys, E Ink refresh timing, or
installation of this worktree candidate. A newer test-only ARMHF bundle passed
927 Reader tests, 14 package tests and four layout probes, but that newer bundle
was not used for this Documents sweep and was not installed. See the
[execution plan](execution-plan.md#current-host-and-device-integration-gate)
for its separate test and offscreen profile evidence.

## Reader 0.1.24 candidate on the Oasis

On 23 November 2026, the 0.1.24 static ARMHF candidate was tested from a
temporary directory on the awake Oasis. Its SHA-256 is
`fd55cce42e733bbe5ac3a2304df226e1684198df4e9a868998db76bc9678078b`. The
installed Reader and Launcher binaries were not replaced. The on-device suite
passed 946 Reader tests (62 ignored because they require private or
device-provided inputs), 14 package tests and four layout probes. The Reader
suite completed in 283.73 seconds.

The same candidate's normal picker diagnostic checked 88 supported file
candidates under Documents using isolated scratch profiles. Seventy-four opened,
advanced one page and resumed after restart: 31 EPUB, 16 AZW3, two MOBI, 19
PDF, four TXT, one PNG and one JPEG. Three protected AZW and ten KFX files
returned Kindle Library guidance. One additional PDF stopped at a possible
password prompt; this run does not establish encrypted-PDF acceptance. No
personal filenames, text, or screenshots were retained. Test folders and
profiles were temporary; the owner's documents and Reader data were unchanged.

The framebuffer showed an existing reading session with Kindle quick settings
open. It was left undisturbed. This run covers device-side automated suites and
the software picker/render/resume path, not foreground touch, physical page
keys, E Ink refresh behavior, or visual acceptance of the new installed app.

## Reader 0.1.25 release on the Oasis

On 24 November 2026, the Reader 0.1.25 GitHub release workflow completed
successfully. The armhf ZIP SHA-256 is
`678f42f3e44b1f32a08526c819fda4c7e3cf8efc9a714f4613383a59f274e4db` (21,252,010
bytes); the installed release binary SHA-256 is
`04ac6a55c96982737bfff8e41652d7a4a3761cdd5ce7f5ccfe324c89ef4a6c06`.

The exact release binary's `--check-document` diagnostic selected and opened a
real EPUB from Documents using a fresh scratch profile. The first frame took
2,174 ms and contained 198,331 ink pixels; a forward page changed the frame,
Continue reading restored the saved page, and the process reported 51,908 KiB
peak memory. The scratch profile was removed. The source document, Launcher and
user reading data were not changed. This automated offscreen diagnostic does not
claim foreground touch or visual acceptance.

## Reader 0.1.26 release on the Oasis

On 24 November 2026 the [GitHub release](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.26)
passed CI and its ARMHF ZIP matched the published checksum
`a21256b97a4fa22799473b3195ec190da37a5678d5978fe3f6ddedb3472c7428`
(21,246,669 bytes). It contains the application and documentation licences.
The same asset is available through the
[Store listing](https://primitiveos.github.io/kindle-primitive-catalog/v1/apps/primitive-reader.json).
The verified Store install transaction updated Reader on the Oasis 3.
The installed binary SHA-256 is
`d597774be791050cd74bf7a4f9cef044bb7ad1c0ca9f6df75ceb4997ac81de06`.

The release executable used the normal picker, software page input, rendering
and a fresh profile to exercise six existing Documents files to completion.
The source file hashes matched before and after these runs.

| Format | Opened, advanced and resumed | First frame |
| --- | ---: | ---: |
| EPUB | 2 | 2.00–2.08 s |
| PDF | 2 | 0.44–16.22 s |
| AZW3 | 1 | 0.99 s |
| MOBI | 1 | 2.16 s |

An additional one-page PDF opened and resumed. Its forward action reached the
end-of-book prompt, and the footer remained at Page 1 of 1, as expected. The
foreground session belonged to Primitive Browser during installation.
The [release footer screenshot](screenshots/footer-release-oasis.png) uses the
exact release binary on the Oasis with an authored EPUB and offscreen rendering.
It shows the hyphenated caption, progress track and Exit control. E Ink refresh
timing and a foreground Reader session remain separate checks.

## Reader 0.1.28 release on the Oasis

The [0.1.28 GitHub release](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.28)
passed the release workflow on 24 November 2026. The ARMHF package contained
both licenses and matched its published SHA-256
`d47fffe805057883fd5cbcec6bc8c47348766d7936976750b9eecc0261b11789`
(21,281,448 bytes). The verified Store install transaction completed on the
Oasis 3. Its installed executable matched the published binary SHA-256
`8fe18ac477ef3006d47a825bd3bc42294bda48cd0ab9efc2248bf9dcbc16bf7f`.

The installed binary selected one existing book of each format through the
ordinary picker. Each file opened, rendered a visible page, advanced and
resumed after reopening. Reader used an isolated profile for each format and
the original file checksums matched before and after.

| Format | First frame |
| --- | ---: |
| EPUB | 1,535 ms |
| PDF | 18,970 ms |
| AZW3 | 1,675 ms |
| MOBI | 1,933 ms |

A separate ARMHF test bundle passed 998 Reader library tests, 14 EPUB package
tests and four layout tests on the Oasis. It skipped 62 optional tests. The
three [Contents captures](searchable-contents.md#evidence) were identical to
the desktop simulator. These checks ran offscreen with the foreground
Browser session open. Physical input and E Ink refresh are separate checks.

## Reader 0.1.29 release on the Oasis

The [0.1.29 GitHub release](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.29)
passed CI. The ARMHF package included both licenses and matched its published
SHA-256 `16f8a8d021e05d8d433179bdaed486000aa086cc7b24513a4361eef27c4bb850`
(21,296,290 bytes). The Oasis 3 installed it with the Store transaction. Its
installed executable matched release SHA-256
`2a65a4547d7762733c33d7f70dd2a4d3b73c204a30739920f08200bea8159372`.

Four existing Documents files opened, rendered, advanced with a software
page event and resumed after reopening. The originals retained their hashes.

| Format | First frame |
| --- | ---: |
| EPUB | 1,521 ms |
| PDF | 19,128 ms |
| AZW3 | 1,972 ms |
| MOBI | 1,925 ms |

[Chapter and Contents verification](chapter-navigation.md) records the 1,009
native tests and the Mac and Oasis frame comparison. Test profiles were
isolated. Foreground physical input and E Ink timing remain separate checks.

## Reader 0.1.30 release on the Oasis

The [0.1.30 GitHub release](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.30)
passed CI. Both licenses were present in the verified ARMHF package
(SHA-256 `aa60eded61d3a3e8285d22bcd362ac60ff15c5f136cd55be1acf64d12fe9495e`,
21,298,825 bytes). The Store transaction installed the release on the Oasis.
The installed executable matched release SHA-256
`06a8aa716b2e8bbe06cfeb8f49d2d3f1229ac91c67be84afb2a0a98bdb516a7b`.

An EPUB, PDF, AZW3 and MOBI already in Documents each opened through the
normal picker, advanced, and resumed. Their original file hashes matched
before and after. First frames took 1,489 ms, 18,972 ms, 1,943 ms and
1,918 ms respectively. [Chapter settings](chapter-level-settings.md)
records the complete native and simulator evidence.

For a single file under an authorized Documents root, run the example with a fresh
scratch directory and the same Noto Sans fonts as Launcher.

```sh
READER_VERIFY_PICKER=1 cargo run --locked --release --example verify_document -- \
  "$documents_root" "$book_path" "$new_scratch_directory" "$regular_font" "$bold_font"
```

The installed production binary exposes the same diagnostic, including the picker,
so release checks can exercise the exact executable from Store. The scratch directory
must be new. The caller can remove it after reviewing the result.

```sh
/mnt/us/reader/bin/reader-kindle --check-document "$book_path" "$new_scratch_directory"
```

Set `READER_VERIFY_SPINE=sample` to check the first, middle and final spine entries
and paint their first, middle and final pages. Use `READER_VERIFY_SPINE=all` to
paint every page of every entry. An ordinary open skips these diagnostics.
Fixed-layout EPUBs count successfully decoded pages and painted frames. The
reflow audit also reports the following evidence.

| JSON field | Interpretation |
| --- | --- |
| `failed_indices` | A spine index failed to open, lay out or paint. Indices start at zero. |
| `genuine_reflow_sections`, `recovered_placeholder_indices`, `empty_body_indices` | A section contains original text or illustrations, a recovery notice supplied by Reader, or a verified attribute-free and structurally empty source body, respectively. `empty_layout_with_markup_indices` records other empty layouts separately for review. |
| `mapped_chars` and `source_chars` | Counts of normalized, non-whitespace source characters covered by line byte ranges and total such characters in genuine sections. `missing_glyphs` separately counts shape results without a glyph in the supplied fonts. |
| `source_graphics`, `decoded_graphics` and `placeholder_graphics` | Image occurrences identified by the reading source model, decoded previews available to the renderer and fallback boxes, respectively. Repeated placements count individually. |
| `located_fragments` and `source_fragments` | Counts of extracted fragment anchors that resolve to a page and the total extracted from genuine sections. |
| `exact_page_anchors` and `inspected_page_anchors` | For each tested page with a reading anchor, the checker resolves the anchor back to a page and compares the index. |
| `painted_pages_with_ink` and `painted_pages` | Page bitmaps with at least one nonwhite pixel and all successfully painted pages, including blank leaves and recovery notices. |
| `issues` | Per-section counts for missing mappings, artwork, glyphs or page anchors. White pages inside genuine chapters can arise from whitespace. Up to eight zero-based example pages identify each blank-frame or anchor mismatch. |

The JSON contains counts and numeric positions. The scratch profile can contain
book metadata and stays local. [Audit implementation](../src/document_check/spine.rs)

On a device, `READER_VERIFY_SPINE=all-sections` checks the source and layout of every chapter and paints its first, middle and final page. Use `all` to inspect every page boundary.

## Whole-spine verification on Mac

Ten anonymized EPUB entries from Kindle Documents went through the production
`reader-kindle --check-document` command with `READER_VERIFY_SPINE=all`, 1264-pixel
width and the Launcher Noto Sans regular and bold fonts. Entries 035 and 065 are
byte-identical copies at different picker locations. The counts below include
both. All 315 spine entries opened and every one of the 8,575 pages painted.

| Entry | Original sections / entries | Pages painted | Source characters mapped | Images decoded / referenced | Fragment anchors located | Missing glyph instances |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 031 | 24/24 | 1,134 | 1,222,865 | 15/15 | 599 | 0 |
| 032 | 11/11 | 1,156 | 1,226,557 | 14/14 | 0 | 0 |
| 033 | 12/12 | 403 | 357,788 | 31/31 | 149 | 63 |
| 035 | 33/33 | 411 | 372,040 | 25/25 | 353 | 0 |
| 046 | 31/31 | 1,207 | 1,184,691 | 79/79 | 24 | 0 |
| 052 | 21/21 | 366 | 419,252 | 39/39 | 1,193 | 2 |
| 053 | 48/49 | 1,973 | 2,102,842 | 1,726/1,757 | 3,259 | 555 |
| 055 | 40/42 | 1,151 | 1,071,808 | 1/1 | 141 | 0 |
| 061 | 59/59 | 363 | 328,305 | 64/64 | 9,017 | 0 |
| 065 | 33/33 | 411 | 372,040 | 25/25 | 353 | 0 |
| **Total** | **312/315** | **8,575** | **8,658,188** | **2,019/2,050** | **15,088** | **620** |

- The checker mapped every normalized non-whitespace character and resolved all
  15,088 extracted fragments. All 8,571 available page anchors returned to their
  original pages. Entry 053 previously restored two illustration pages to the
  first image in the paragraph. Saved anchors now retain the image ordinal.
- In 033, 052 and 061, valid PNG and JPEG illustrations near the ends of chapters
  previously showed placeholders. Reader now budgets the retained grayscale
  previews separately from decoding the full source. Pictures keep their
  display and touch geometry when their cached resolution is reduced.
- In 053, index 21 rendered a recovery notice. It contributes zero source
  characters, illustration references or fragment anchors. Independent ZIP
  validation found 41 unavailable image members. The surviving chapters
  reference 31 of those missing images and 1,726 intact image occurrences.
  The renderer decoded all 1,726.
- In 055, indices 40 and 41 are two literal empty body elements. Both pages
  stayed empty. Two further white frames occurred inside genuine chapters at
  (032, section 2, page 102) and (061, section 11, page 10). The former
  contains a trailing whitespace line and the latter has no visible line
  in the final layout region. These are zero-based positions.
- Character coverage records layout mapping. The 620 missing-glyph instances
  are a separate font-coverage limitation with the supplied fonts. This Mac
  pass measures the software renderer and input handlers. Physical display
  behavior is covered by the separately identified Oasis runs.

### Trailing reflow pages

A follow-up Mac run checked the two all-white trailing views from the whole-spine table. In 032, the final 113.9 pixels held a terminal hard break and an empty layout line. In 061, the paragraph's nominal height extended 0.44 pixels past the page boundary after its last visible line. Reader now ends pagination at the final line, illustration, annotation or paragraph background with visible content. End-of-chapter links and saved text positions still resolve.

The two affected chapters produced 102 and 10 pages respectively. All 112 pages showed ink, all 112 page anchors returned to their original pages, all 214 source fragments resolved and all 120,896 non-whitespace characters mapped to laid-out lines. The two intentionally empty entries in 055 each continued to paint a single white page. Authored regression tests also check multiple terminal breaks, a trailing anchor and shaded whitespace.

## Reader 0.1.5 on the Oasis

| Format | Files attempted | Opened and drew a page |
| --- | ---: | ---: |
| EPUB | 31 | 14 |
| AZW3 | 16 | 0 |
| PDF | 4 | 3 |
| TXT | 4 | 3 |
| Legacy MOBI | 2 | 1 |
| Amazon AZW | 3 | 0 |
| KFX and related Kindle files | 10 | 0 |

The Kindle's Store receipt identified the installed Reader as 0.1.2. For the comparison above, the latest public source at the beginning of the run (0.1.5) was rebuilt and run on the same Oasis. The baseline used the ordinary document opener. It covered 70 original files including
nested Downloads and dictionary folders. Earlier generated test books and Kindle
sidecar directories were excluded. Of the 17 EPUB failures, nine involved the first
chapter, four involved XML declarations and four involved ZIP packaging. A reference is counted as an EPUB only when the engine identifies its format as
EPUB. CREngine can return a successful load and a page count after falling back
to plain text on a damaged ZIP. All three AZW headers report encryption. Reader
has no KFX engine.

## Reader 0.1.6 on the same Oasis

| Original documents | Opened, rendered and resumed | What changed |
| --- | ---: | --- |
| EPUB | 31 of 31 | Read publisher XML declarations, compressed or displaced mimetype entries, recoverable ZIP directories, blank leaves, and SVG covers with linked pictures. Display an invisible character safely in a picker label while preserving the underlying filename. |
| AZW3 | 16 of 16 | Read unencrypted KF8 books with auxiliary flows, larger text sections, producer-specific markup and embedded illustrations. |
| Legacy MOBI | 2 of 2 | Bound decompression and paginate long text into manageable sections. |
| PDF | 4 of 4 | Read the large scanned document within the 128 MiB source budget. |
| TXT | 4 of 4 | Recognize an HTML text export containing terminal color escapes. |
| Protected AZW | 0 of 3 | The original Palm database headers report encryption. The Documents list points to Kindle Library. |
| Kindle KFX | 0 of 10 | The Documents list identifies KFX and points to Kindle Library. |

The picker results above used the original files under Documents, including nested
folders. The 96,319,018-byte PDF took 45.7 seconds to open in the instrumented run.
Reading the file took 8.2 seconds and rendering the cover took 35.6 seconds. On
reopen, rendering the following page took 0.29 seconds. Further PDF cover work
has a measurable target.

In one damaged EPUB, 1,418 of 1,460 archive members could be validated. Forty-one
images and one chapter were missing or damaged. Reader preserves the original
chapter numbering, displays a message for that missing chapter and allows the
next turn. A successful open cannot reconstruct bytes that are absent from the file.

A different EPUB has 42 linear spine entries. Reader rendered all 40 entries
containing text or artwork. The final two first frames were blank. Inspection of
their original CRC-checked XHTML entries found two byte-identical 269-byte files,
each with an empty title and a body containing one whitespace character. Neither
entry contains an image, stylesheet, script, or any element inside the body.
The blank frames match those source pages.

The extended picker run also exercised 36 JPEG files, 23 individual XHTML chapters
and the complete extracted publication. All 60 opened and resumed. In total, 117
of the 130 entries rendered and restored the same page body; 13 were identified
as Kindle Library formats or protected books. The same source revision passed 324
library tests, 14 package tests and four layout tests on both the Mac and the
Oasis. Those automated suites use the project’s own fixtures.

## What KOReader does with the same files

The tagged front end chooses a provider by registered extension, saved preferences
and provider weight. CREngine registers EPUB, MOBI, AZW, TXT and XHTML. MuPDF
registers PDF and JPEG. An engine-only test of an AZW3 file uses an explicitly
selected provider; the tagged front end has no stock AZW3 registration.
[Provider selection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/documentregistry.lua#L91-L141)
· [CREngine registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1580-L1628)
· [MuPDF registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L380-L410)

- **Archive recovery.** CREngine's ZIP code first looks for the central directory
  and can retry against local headers. On the damaged 49-entry spine in this
  corpus, its binding returned success but identified the book as *Plain text TXT*
  with 7,962 pages. Reader recovered the EPUB package, displayed the 48 surviving
  chapters and kept the missing chapter's position. The binding considers the
  document loaded when it has a DOM, which makes a page-count-only comparison
  misleading. [ZIP reader](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstream.cpp)
  · [CREngine Lua binding](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L690-L707)
- **PDF drawing.** The installed MuPDF engine recognized 1,041 pages in the
  same 92 MiB PDF and painted the cover in 8.42 seconds including open. Reader's
  comparable instrumented run took 45.7 seconds including its picker, identity
  and first paint. These are single observations with different startup scopes.
  MuPDF paints the requested rectangle into a grayscale pixmap. The cover
  contains over 32 million source pixels across three CMYK JPEGs. Reader's
  measured bottleneck is color conversion during the first paint. [MuPDF drawing](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/mupdf.lua#L716-L749)
  · [KOReader PDF dependency](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/mupdf/CMakeLists.txt#L125-L132)

## Checking past the opening page

The published 0.1.7 binary was then checked in place through the Oasis Documents picker. All 117 readable entries opened, turned where available, and resumed at the saved reading position. Across 72 spine checks, all 212 sampled sections loaded and all 349 available page anchors matched. The three protected AZWs and ten KFX files each returned their Kindle Library guidance. The test included 31 EPUBs, 16 AZW3s, two MOBIs, four PDFs, four text files, 36 JPEGs, 23 XHTML files and an extracted EPUB. The two interior chapters which had failed in 0.1.6 both passed.

The public 0.1.6 ZIP was installed in place on the Oasis. Its 23 files matched the release checksum manifest and the three existing Reader data files retained their checksums. The installed executable rendered and resumed all 117 readable entries. In the same run, it tried 212 first, middle or final spine entries, including entries reached from the extracted book's separate XHTML files. Two sampled chapters in one EPUB failed after its opening page had passed.

In this book, a font's ascenders and descenders overlapped the next line's nominal bounds. Reader treated an entire long paragraph as inseparable at a page break. The revised layout uses the font's natural line spacing for a paragraph whose overlapping lines would otherwise fill a whole screen. The Oasis rendered the first and last pages of **all 29 chapters** and restored the page after a fresh start. The two initially failing chapters accounted for 34,478 non-whitespace source characters on the Mac; all of them mapped to laid-out lines. On this same Oasis, KOReader identified the original as EPUB, laid it out in 844 pages and painted 30 distinct first, last or contents destinations. The page counts depend on the selected fonts and margins. CREngine's page splitter explicitly distinguishes an oversized single line from a group of lines that can be separated. [Upstream page splitting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvpagesplitter.cpp#L539-L600)

With the combined layout and image fixes on the Mac, all 932 pages of this 29-chapter book drew visible content. All 754,940 normalized non-whitespace characters, 144 illustrations, 464 fragment destinations and 932 page anchors passed their respective checks. The chosen fonts produced zero missing glyph instances.

The optional check `READER_COMPARE_CONTENTS=1` makes the installed KOReader comparator paint every distinct contents destination plus the opening and closing pages. Only counts, detected format and timings are reported. [Reference tool](../scripts/koreader-document-check.lua)

## Comparable page images

Both views show the same generated long-chapter publication, using Noto Sans at
42 pixels. The KOReader image is CREngine's 1264 × 1606 document surface on the
Oasis. The Reader image is the 1264 × 1680 app simulator including its footer.
Paragraph indentation, line breaks and pagination differ. These captures make
those differences visible without publishing a page from the owner's library.

| KOReader on Oasis | PrimitiveOS Reader |
| --- | --- |
| <img src="screenshots/koreader-long-chapter.png" width="245" alt="KOReader rendering the opening of the generated long chapter"> | <img src="screenshots/reader-long-chapter.png" width="245" alt="PrimitiveOS Reader rendering the same chapter in its reading interface"> |


## Oasis firmware font coverage

A follow-up Mac run shaped all 82 spine entries of the three originals that contributed the 620 missing glyphs in the whole-spine table. It used the same Launcher regular and bold fonts and the installed Oasis 3 firmware fonts in Reader's preference order. Every missing character belonged to Unicode's Common script. The source contained 556 mathematical symbol occurrences, 63 other symbol occurrences and one nonprinting C1 control.

| Font configuration | Missing printable glyphs | Missing control glyphs |
| --- | ---: | ---: |
| Launcher Noto Sans regular and bold | 619 | 1 |
| Launcher fonts plus the existing Noto Naskh Arabic and TBGothic firmware fallbacks | 64 | 1 |
| Same configuration with the installed Code2000 last | 0 | 1 |

Reader discovers Code2000 when the firmware provides it. Fontique maps the firmware files on demand. Removable user fonts keep their existing in-memory handling. The check retained source characters and counted the control's missing glyph explicitly. The regression test uses two redistributable fixture fonts, and the release contains no Oasis firmware fonts.

On the Mac, an authored EPUB's first-page layout and raster took a median of 19.80 ms with the previous stack and 19.55 ms with the additional file-backed fallback. Each configuration ran 21 times, alternating their order after three warm-up rounds. These are warm-filesystem host timings. The Kindle foreground stayed undisturbed during this comparison.

## Table columns in EPUB and AZW3

One original EPUB and its AZW3 edition contain a seven-row, three-column table with three headers and 21 nonempty cells. On the Oasis, KOReader's CREngine drew the EPUB headers at x = 142, 534 and 922 on the same baseline. In Reader 0.1.7 all three headers began at x = 54 on separate lines in both editions. This displaced each value from its heading. [CREngine table cells and spans](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L417-L539)

Reader now measures and places the cells of a row within the page width. Each source block retains its own text positions for links and selection. With 42-pixel type in the Oasis-sized simulator, both editions placed their three headers at x = 63, 448 and 833 at a common height. Tests use two viewport widths, hit-test text in all three columns, and verify the first paragraph after a multi-paragraph cell. The grid supports cells spanning rows or columns, empty cells, row groups, and right-to-left column ordering. Percentage widths in column declarations shape the grid. A merged cell reserves its column through its last row, including a span to the end of a row group. Very narrow layouts, nested tables and layouts without a safe page break retain linear reading order. CSS-defined borders and intrinsic column sizing require further work. [Authored simulator example](screenshots/publisher-table.png)

## Full-page playback with publisher list and table layout

The Mac verifier opened eleven entries from the Oasis and rendered all 344 spine positions at reading size, including a duplicate edition in another folder. Across 9,503 rendered pages, 9,501 contained ink. The two white pages are empty leaves present in the publisher's source. All 9,413,128 normalized non-whitespace characters mapped to lines, all 15,552 fragment destinations resolved, and all 9,500 available page anchors returned to the same page. The check used Noto Sans Regular and Bold. Character-to-line coverage is independent from whether a particular glyph exists in the selected fonts.

The damaged EPUB retained its missing-chapter notice and rendered all 1,726 available image placements out of 1,757 in the surviving markup. Across all eleven entries, 2,163 of 2,194 image placements rendered. The 31 unrecoverable placements refer to absent resources in that same EPUB. All 1,947 pages rendered in its surviving and recovered spine positions contained visible content.

One of this book's five-row tables declares 30%, 40% and 30% columns. A heading spans all three columns and the left label spans four rows. Ten word, caret and page-location probes in the original matched their cells. Changing the columns to 20%, 50% and 30% in an isolated comparison copy moved the middle column 116 pixels to the left while leaving the right column at the same x coordinate. Eight original tables in the same chapter combine both kinds of span. Checks across 89 rows verified 186 native-text cell positions and all 17 tested page anchors. One empty or non-word cell had no word to hit-test.


On the Oasis, the 0.1.7 baseline painted identical first-page images for the source list and two altered copies. In the 0.1.8 candidate, setting the first ordinal to 81 changed 1,548 of 2,123,520 pixels. Switching to bullet markers changed 326,050 pixels. All three versions mapped the same 201,498 source characters and returned the same single search match. The physical EPUB and AZW3 table-header probes both returned x = 63, 448 and 833 on a single row in their respective pages. The source file hashes matched the independent Mac check.


The merged-table case was repeated offscreen on the physical Oasis using its production fonts. The ten word and caret probes and both page anchors matched the independent Mac build. In the original table the second and third columns started at x = 410 and 872 on three subsequent rows, and the three-column heading reached x = 932. An isolated width-control copy moved the middle column by −116 pixels and the last column by zero on both machines.

The list audit also exercised the 103 text-bearing list sections found in ten EPUB and Kindle-format files on the Mac. Each section painted its opening page and accounted for all of its normalized source characters, totaling 1,590,530 characters across 1,729 layout pages. The sample includes different editions of the same publication. The reusable [layout verifier](../examples/verify_reflow_geometry.rs) reports aggregate positions and pixel differences for a list or table in a supplied original and isolated controls.

## Following coordinates in PDF links

Three original PDFs containing 2,148 pages were checked on the Mac. The audit read every link annotation and outline item. It compared exact physical page numbers against the PDF page references, and sampled selectable words and their rectangles on 95 pages. The originals contained 50 valid local links, 11 external links and 51 outline entries with valid page targets.

In the 1,066-page PDF, all 50 valid local links specify both coordinates of an XYZ destination. Nine specify a point 72 points from the left edge and 144.46 points below the top. The others specify the page corner. At 300% zoom, Reader 0.1.8 opened the correct page for every link, but the selected viewport placed all 50 target points offscreen. The revised Reader placed all 50 within one rendered pixel of the upper-left corner in both paged and continuous modes. KOReader also passes a resolved PDF link's coordinates to its page or scroll controller. [KOReader link coordinates](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/mupdf.lua#L639-L668) · [KOReader destination positioning](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L1147-L1165)

Another 27 contents links and 27 outline entries in the same file specify a null target page. MuPDF's fallback sends those entries to the first page. Reader identifies the missing destination as unavailable. Two further outline entries contain no destination. All 51 well-formed outline targets and all 50 well-formed link targets resolved to their exact source page numbers.

The mixed-orientation PDF has a landscape cover and 1,040 portrait pages. With a manual crop applied, simulated forward and back button presses crossed that boundary, the visible words tested on the second page remained selectable, and returning to the cover reproduced its first grayscale frame byte for byte.

An independently authored two-page PDF exercises the ordinary tap, same-page link, history navigation, and reopen paths. Separate regression tests cover named and explicit destinations, rotated crop boxes, and paged and continuous reading at 300% zoom. Reader keeps the current zoom. A missing coordinate retains page-only navigation. [Oasis-size simulator after following a link](screenshots/pdf-link-destination.png)

### Physical PDF link taps at 300%

Two independently built ARMHF verifiers ran on the Kindle Oasis, opening the original 1,066-page PDF in Documents. A test harness positioned each source annotation in view and tapped it through Reader’s ordinary input handler. It measured the destination in the resulting offscreen viewport. Each run used its own reading profile. The SHA-256 of the 12,698,773-byte source matched before and after all four runs.

| Build and reading mode | Correct physical page | Authored point visible | Within one pixel of intended corner |
| --- | ---: | ---: | ---: |
| 0.1.8, paged | 50/50 | 0/50 | 0/50 |
| 0.1.8, continuous | 50/50 | 0/50 | 0/50 |
| 0.1.9 candidate, paged | 50/50 | 50/50 | 50/50 |
| 0.1.9 candidate, continuous | 50/50 | 50/50 | 50/50 |

The candidate’s largest error on either axis was below 0.00004 pixels in paged mode and 0.258 pixels in continuous mode. All nine interior points met the one-pixel criterion in both modes. The reference manifest was decoded separately on the Mac. The Oasis matched all 86 annotations in this PDF, including 27 missing targets and nine external links, and matched all 80 outline rows. Each run also tapped one missing target and one external link and verified the message and source page. The tested revisions were `2420137` and `2def76c`.

## Saved locations across the 0.1.8 upgrade

Reader 0.1.9 reopened the stored 0.1.8 anchors for eleven original EPUB entries. On the Mac the test restored a painted page at two widths, with automatic and authored word breaks. All 33 restorations kept the publication, chapter and source revision identifiers. A change in displayed page number is expected when line breaks or the available width change.

With the 0.1.9 default layout the same eleven entries produced 9,436 pages across 344 spine positions. All 9,413,128 source characters mapped to layout, 2,163 available illustrations rendered, and all 9,433 available page anchors returned to their page. The two blank leaves and the 31 missing illustrations have the same source explanation recorded above.

## Finding text and saved highlights at high zoom

The Mac verifier opened two original PDFs with 1,066 and 41 pages. For each PDF, an independent decoder identified six words near the edges and center of several pages. The check selected each word through Reader’s search results, then followed its saved highlight after closing and reopening an isolated reading profile.

| Reading mode | Search matches visible before | Search matches visible after | Reopened highlights visible before | Reopened highlights visible after |
| --- | ---: | ---: | ---: | ---: |
| Fit page | 12/12 | 12/12 | 12/12 | 12/12 |
| Paged at 300% | 2/12 | 12/12 | 2/12 | 12/12 |
| Continuous at 300% | 2/12 | 12/12 | 2/12 | 12/12 |

Reader retains the current zoom and brings the matched words into view. A multi-line result marks the individual words. For a passage larger than the screen, the first word provides the starting position. A saved highlight can be followed and retraced even when both positions are on the same page. Authored tests cover a manual crop, quarter-turn rotations, a passage outside the crop, back and forward navigation, and reopening. [Oasis-size simulator showing a search match at 300%](screenshots/pdf-search-target.png)

KOReader forwards a saved highlight’s position to its page controller. Its PDF search highlights matching boxes and optionally fits the result page to the screen. [KOReader bookmark navigation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L496-L512) · [KOReader PDF search](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L403-L428)

The same search and saved-highlight check ran offscreen on the physical Oasis with the original 1,066-page PDF. At fit page both builds showed all six sampled words. At 300% the previous build showed one of six through each action in each reading mode. The revised build showed all six words completely in both actions and both modes. The verifiers derived search terms from independently measured word coordinates, saved highlights in separate scratch profiles, and reopened those profiles before following each highlight. The source file’s SHA-256 digest matched before and after both runs. The tested revisions were `07203d2` and `b8adeae`.

The Mac also repeated the 50 local PDF link taps on the revised build. All 50 XYZ destinations, including the nine interior points, landed within one pixel of the intended corner at 300% in both modes. The largest axis errors were 0.000032 pixels in paged reading and 0.258 pixels in continuous reading.

### Phrases that wrap in a PDF

Four original PDFs contained 2,377 pages in total. The Mac check sampled three interior pages per document. For each page, MuPDF independently located four consecutive words split over two lines and a separate four-word phrase on one line. Its results occupied two physical rectangles and one respectively. Reader’s word coordinates contained all 96 independently measured word centers.

| Query | Pages with the expected result before | Pages with the expected result after |
| --- | ---: | ---: |
| Four words on one line | 12/12 | 12/12 |
| Four words spanning two lines | 0/12 | 12/12 |

KOReader searches the next word box when a query continues on the following line and marks each matched word. Reader uses the source word boxes and preserves UTF-8 offsets while matching across adjacent lines in the same column. The authored tests cover double-spaced lines, folded accents, a large gap between paragraphs and rows containing two unrelated columns. On the authored chart, all four marked words fit completely at 300% in both paged and continuous reading, and Previous location returns to the source page. [KOReader’s fixed-page phrase matcher](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1502-L1607) · [Oasis-size simulation of a wrapped search match](screenshots/pdf-wrapped-search.png)

Each real wrapped hit spanned more than the screen width at 300%. On the Mac, selecting each of the 12 hits in both modes retained all four word marks, showed both words at the beginning of the match completely and returned to the previous page. The other two words remained marked at the far side of the following line.

An offscreen Oasis run repeated all 24 Reader jumps with the same geometry and history results. In a separate process, the installed KOReader Lua phrase matcher searched the same pages using its own MuPDF word boxes. It located all four independently measured word centers for every wrapped and same-line control. All four original SHA-256 digests matched before and after Reader’s checks. Both verifiers ran against the books in Documents with separate test state.

## Reading positions saved beside existing books

KOReader places metadata beside a document by replacing its last filename suffix with `.sdr` and naming the file `metadata.<original suffix>.lua`. Reader checks that exact location, including suffix case. With a copied file and adjacent metadata both present, the book’s normal import opens the adjacent file and the separate Reading data action opens the copy. Each route previews the destination before applying it. [KOReader’s sidecar naming](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/docsettings.lua#L118-L145)

The Oasis collection contained eight metadata files, six for EPUB and two for PDF. Seven could be paired with a readable original in the collection. The Mac test opened the matching 1,041-page PDF and its original KOReader metadata in an isolated Reader profile. Controls found the adjacent metadata, preview left the current page in place, Apply opened saved page 32, and Previous location restored the source page. The metadata remained byte-identical. An authored test separately checks mixed-case suffixes, multiple formats sharing one directory, an explicit copied-file override and preview without moving.

## Restoring KOReader EPUB positions

Six unmodified EPUBs on the Oasis had KOReader sidecars with a sampled edition checksum and a saved CREngine pointer, all from DOM version 20240114. None had a CFI. On the Mac, Reader verified each checksum and located all six positions in their original spine chapters. The chapter key, source revision, reading block and UTF-8 offset come from the original book. The authored fixture includes two identical sentences so selecting its second occurrence also checks that text alone cannot identify the position. [Import preview](screenshots/koreader-position-preview.png) · [Reading after applying the saved location](screenshots/koreader-position-page.png)

In an independent run on the Oasis, installed KOReader returned the text reached by stepping ten words backward and forward around each saved pointer. Reader matched all twelve windows on the Mac after removing Unicode whitespace from both engines’ output. One preceding window was empty because the saved position was at the beginning of the visible text. Three positions were text-node offsets. Three were empty element boundaries, each of which resolved to the same following passage as KOReader. All six used the saved structural pointer. The saved percentage did not contribute to positioning.

CREngine numbers the EPUB spine fragments and sibling nodes starting at one. Its normalized pointers omit internal layout boxes. Text offsets count Unicode scalar values after its XML text preprocessing. Reader supports these source-backed operations, checks for ambiguous sibling selectors and retains the existing reading place when a pointer cannot be resolved. The bounded subset excludes older DOM versions, nonzero element-child offsets, text indices affected by CREngine splitting long text nodes, and styled whitespace with ambiguous offsets. [CREngine EPUB fragment assembly](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1846-L1923) · [Normalized pointer resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L11355-L11429) · [Initial whitespace in block elements](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L8953-L8974)

An independent ARM build from `71bbca0` ran the normal Preview and Apply actions offscreen on the same Oasis. Each original EPUB opened with its own scratch profile. Preview preserved the current page, Apply opened the page containing the verified anchor, and reopening the book restored that page in all six cases. All twelve KOReader text windows matched on the Oasis, totaling 573 UTF-8 bytes after whitespace removal. The source files totaled 10,049,352 bytes and the six original sidecars totaled 21,774 bytes. Full SHA-256 digests for all twelve files matched before and after the run. The same check passed on the Mac. These six imports used the structural pointer directly.


## KOReader marks saved alongside an EPUB

KOReader records a reflowable page bookmark as a source pointer in `page`. A highlight adds its style, the two endpoints `pos0` and `pos1`, the selected text, and an optional note. Reader resolves both endpoints against the same edition, extracts the intervening source text and compares it with the saved quote before offering to import it. The preview counts unsupported records, including edited quotes that disagree with the document and fixed-page coordinate records. A page bookmark with its own separate note also counts as unavailable. [KOReader annotation model](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerannotation.lua#L65-L83)

Three original EPUBs in the Oasis collection had annotations in their adjacent metadata. Two contained highlights and one contained a page bookmark. The installed KOReader returned the exact saved passage between both pairs of endpoints. On the Mac, Reader offered all three original marks, applied them twice in isolated profiles and reopened exactly one record in each book. The contents of every publication and its metadata matched their pre-test hashes. The public screenshot and the regression for a repeated word use the authored Field Notes fixture.

The Oasis repeated Preview, Apply twice and reopen against each of those three EPUBs using isolated Reader profiles. Each reopened with one mark. The original books, KOReader metadata, active Reader data and Browser session retained their hashes after each run.

## PDF link addresses

Three original PDFs contained sixteen URI annotations. Twelve recorded web addresses, one a mail address, one a file URI, and two relative addresses. A fourth original contained no URI annotations. On the Mac, all sixteen link rectangles opened the same address that the independent PDF parser extracted. Displaying a QR code and copying an address preserved the exact page, viewport and location history.

KOReader’s link controller offers Copy and QR code on this device. Its optional browser action depends on the device URL handler. On the tested Kindle, the generic device sets that capability to false and the Kindle backend inherits it. Reader likewise keeps the address dialog offline. Paste link inserts a copied address into a Reader search or note field. The compiled QR generator’s MIT notice is included with the application. [KOReader link dialog](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L138-L179) · [QR encoding](https://docs.rs/qrcodegen/1.8.0/qrcodegen/struct.QrCode.html)


## Page geometry and text across four original PDFs

Reader and an independent MuPDF extraction visited all **2,377 physical pages** in the four PDFs from the Oasis. Their page counts and upright dimensions agreed on every page within 0.02 PDF points. Both found alphanumeric text on the same 2,375 pages. Reader checked 1,235,700 word spans for valid source-byte ranges and page coordinates. No page reached its text extraction limit.

The comparison measures page geometry, text availability and selection bounds. Word segmentation and readings of individual glyphs are checked separately using source passages. To repeat the comparison, pass the PDF and a JSON array of `width`, `height` and `has_readable_text` values to `cargo run --example verify_pdf_pages --`. The verifier emits counts and page numbers. [Page verifier](../examples/verify_pdf_pages.rs) · [Reading interaction checks](reading-control-evidence.md)

## Inline illustrations across the original EPUB editions

A resource inventory of the 30 distinct EPUB editions found 29 packages with directly readable central directories. Their manifests declared 706 JPEG, 216 PNG and 82 GIF resources. The bounded recovery path for the remaining damaged edition is described above.

In the 0.1.13 desktop run, three books showed a total of 80 placeholder placements for valid packaged GIF illustrations. All 82 original GIF files have a single frame. The book containing 17 GIF files references 15 of them in the checked page layouts. These 80 placements identify an EPUB decoder gap. The 31 unavailable images in the damaged edition have a different cause. Opening, turning pages and resuming a book can all succeed while individual illustrations still need correction.


## Installed KOReader provider sweep on 22 November 2026

A headless comparison used the installed KOReader release on the Oasis, a temporary cache, the original Documents files and a six-minute limit per book. Of 30 distinct EPUB editions, 29 returned a rendered EPUB document with a page count. The remaining damaged EPUB timed out. An earlier provider check had identified its plain-text fallback. Both MOBI books opened as PDB, and all four PDFs opened through MuPDF with 229, 1,041, 1,066 and 41 pages respectively. File and existing Reader and Browser state checksums matched before and after the sweep. The provider counts cover these 36 publications. Reader’s full Documents open-and-resume checks are recorded separately. [Provider comparison tool](../scripts/koreader-document-check.lua)
