# Opening books from Kindle Documents

The compatibility run uses the owner's existing books on an Oasis 3 with firmware 5.18.2.
The reference is KOReader v2026.07.1, the version installed on this device.
Public records include aggregate outcomes. The original files, names, book text,
screenshots of personal books and saved reading positions are absent from the repository.

## What the test exercises

The device driver runs the release Rust code on the Oasis. It browses folders under
Documents through the normal picker, uses the page button to reach later rows, taps the
file row, renders the first page and sends a forward page-button event. It writes its own
scratch profile, quits and opens Continue reading to compare both the saved position
and the rendered page body. This software path can run while another app is in the
foreground. Physical touch, e-ink timing and sleep are separate checks.

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

Set `READER_VERIFY_SPINE=sample` to paint the first and last pages of the first,
middle and final chapter of a reflowable book. Set it to `all` to exercise every
spine item. Fixed-layout EPUBs use their page decoder for the selected entries.

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

## Comparable page images

Both views show the same generated long-chapter publication, using Noto Sans at
42 pixels. The KOReader image is CREngine's 1264 × 1606 document surface on the
Oasis. The Reader image is the 1264 × 1680 app simulator including its footer.
Paragraph indentation, line breaks and pagination differ. These captures make
those differences visible without publishing a page from the owner's library.

| KOReader on Oasis | PrimitiveOS Reader |
| --- | --- |
| <img src="screenshots/koreader-long-chapter.png" width="245" alt="KOReader rendering the opening of the generated long chapter"> | <img src="screenshots/reader-long-chapter.png" width="245" alt="PrimitiveOS Reader rendering the same chapter in its reading interface"> |
