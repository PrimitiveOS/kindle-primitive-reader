# EPUB Contents and publisher landmarks

Open **Contents** from either reading bar. An EPUB 3 publication can supply a table of contents, a landmarks list, and a list that maps printed page numbers to passages. Reader displays the publisher's table of contents when present. If it is absent, Reader uses landmarks, then printed page numbers. A separate printed page map stays available when the book supplies one. An explicit but empty table of contents can fall back to the book's NCX. This follows CREngine's navigation selection. [CREngine EPUB navigation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1988-L2098)

A publisher can put a plain heading above several linked rows. Tapping the heading opens the first reachable local child passage, including a passage inside a chapter. Reader checks that the chapter is unique in the spine and that a named fragment exists in its reading text. The check can inspect up to 512 KiB per chapter and 8 MiB in total. A group with no confirmed child remains unavailable. Previous location returns to the page from which you opened Contents. [CREngine group destinations](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L220-L284) · [Authored EPUB](../tests/fixtures/landmark-survey.epub) · [Reader screenshot](screenshots/publisher-landmarks.png)

## Navigation, opening position and bookmarks

**Contents** displays book navigation. **Continue reading** restores your saved passage. Opening a new publication starts at the beginning. A publisher's guide entry labelled “text” can point elsewhere in the book. Its presence does not create a personal bookmark or replace your saved reading position. KOReader separately restores reading position, implements Go to beginning and stores the user's bookmark actions. [Saved reading position](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L203-L231) · [Go to beginning](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readergoto.lua#L200-L215) · [Page bookmarks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L370-L421)

## Reference checks

The installed KOReader on the Oasis rendered five authored publications with the same two spine chapters and a later designated text starting point. Every fresh opening displayed its first page. The references below use KOReader's pagination at the test font size. Reader verifies the chapter and publisher fragment because page numbers vary with reading settings.

| Book navigation supplied | Installed KOReader Contents | Local destination |
| --- | --- | --- |
| EPUB 3 TOC, landmarks and page list | Primary entry, page 1 | First chapter, opening |
| EPUB 3 landmarks and page list | Start the survey, page 8. Reference and its child Source notes, page 10 | Second chapter, body and sources |
| EPUB 3 landmarks, page list and EPUB 2 NCX | Same three landmark entries | Same two destinations |
| EPUB 3 page list only | 10 and 21, pages 1 and 8 | First chapter opening and second chapter body |
| EPUB 2 OPF guide only | Two spine entries, pages 1 and 8 | First and second chapter |

All 28 inspected original EPUBs already had a publisher TOC or NCX. Ten declared both a primary EPUB 3 TOC and landmarks. Twelve of 16 original AZW3s contained reconstructed package guides, including HTML contents shortcuts, text starts and front matter. The two AZW3s without an indexed contents table also lacked guide entries. A direct comparison with the installed CREngine PDB provider opened two original AZW3s at page 1, with 301 and 236 pages respectively and no Contents rows from their guides. KOReader's registry has no separate AZW3 guide navigator. Reader uses the actual indexed KF8 Contents when available. [KOReader provider registry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/documentregistry.lua#L91-L150) · [KF8 publisher Contents checks](kindle-format-reading.md)

Native Reader on the Oasis and the Mac simulator agreed on all nine authored navigation rows across four EPUB 3 controls and on the two-spine EPUB 2 fallback. From the reading bars, three ordinary taps reached the designated chapter or passage. Previous location restored the original rendered page every time. Reopening restored the saved reading location and the bookmark count stayed zero. The Kindle and simulator Contents captures differ by one gray level at one pixel out of 2,123,520. The linked screenshot comes from the Kindle's offscreen renderer.

To reproduce the independent controls on the desktop, generate the five public fixture variants and run the focused tests from the Reader checkout.

```sh
python3 scripts/generate-landmark-controls.py build/landmark-controls
cargo test --locked --lib publisher_landmarks
READER_TEST_PUBLISHER_NAV_VARIANTS="$PWD/build/landmark-controls" \
  cargo test --locked --lib publisher_landmarks_match_independent_reference_controls -- --ignored
```
