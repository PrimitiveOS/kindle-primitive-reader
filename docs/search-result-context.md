# Finding a passage in the results list

Open **Find in book**, type a word or phrase and choose **Find all**. The result list shows the matching words in bold within their surrounding text. Tapping a row opens the passage. The physical page buttons move through the list.

| Reading the results | Holding a result |
|---|---|
| ![Search results with the matching word emphasized in context](screenshots/find-result-words.png) | ![The result's page and two-level contents path shown over the same list](screenshots/find-result-context.png) |

Hold a row to see its page and its place in the table of contents. A fixed document uses its page label. A reflowable book uses a publisher or generated reference page when one is available. Without a reference page, Reader shows the current chapter’s screen page, or the chapter number and percentage for a passage in another chapter. Opening the information for another chapter preserves the active reading position. The screen shows the deepest heading in bold. Swipe vertically to read a hierarchy longer than nine headings.

Tap the screen or press either physical page button to dismiss the panel. A small line beside the held result marks the row. Tap that row to open the passage, or continue through the results with the page buttons.

KOReader emphasizes the full matching PDF word and adds a reflowable word's prefix and suffix around a substring match. Its result-hold action obtains a page label and the nested section titles. Its information panel closes on a tap or a key press. Reader uses the source word and the same look-up order. The text renderer shapes the whole row in one paragraph, including mixed-direction text and combining marks. [KOReader result formatting and hold](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L710-L784) · [CREngine word context](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L4052-L4087) · [Chapter-path lookup](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L382-L473) · [KOReader dismissal](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/infomessage.lua#L306-L313)

The screenshots use a four-page harbour register authored for this test. Its result on page four belongs to *Coastal stations › Outer harbour*. The same interface test holds an entry for chapter 48 in an EPUB, dismisses it with the physical back button, opens the exact match, and returns to the starting position. [Reader result presentation and regression tests](../src/app/result_context.rs)

## Oasis 3 comparison

The normal result-row handler on the Mac and in a native ARM test session on an Oasis 3 opened the same passages in four original books. The installed KOReader engines independently found the same numbered occurrence. KOReader’s ordered chapter titles and its full emphasized word matched Reader in all four cases. In both PDFs, a separate Poppler outline and text extraction confirmed the physical page and titles.

| Format | Result number | Section depth | KOReader page | First hold on Oasis |
|---|---:|---:|---:|---:|
| PDF | 43 | 0 | 6 | Under 1 ms |
| PDF | 101 | 2 | 106 | Under 1 ms |
| EPUB | 93 | 2 | 18 | 52 ms |
| EPUB | 143 | 1 | 11 | 25 ms |

The EPUB page numbers in the table identify the independent CREngine layout. Reader resolves the source position and uses the page reference or chapter position described above. The timings cover lookup and preparation of the information panel. They exclude E Ink refresh. In each case, dismissing the panel retained the result row, tapping the row opened the selected text or word rectangles, and returning restored the starting reading location. All original file checksums were unchanged.

The authored result list and information panel each produced identical 1,264 × 1,680 grayscale pixels on the Mac and the Oasis. An independent font-rendering regression on the Oasis checked that bolding a Unicode span preserved the regular-weight prefix. The EPUB reference-label test also exercises a publisher’s Roman page numbers and generated character-count pages across chapters. [Reader’s source/geometry tests](../src/app/result_context.rs) · [EPUB page reference lookup](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L1395-L1429) · [Installed reader’s chapter-path comparison](../scripts/koreader-result-context.lua) · [Verified text renderer](https://github.com/PrimitiveOS/kindle-primtitive-tools/commit/85095a5f28f50e0e2bb45db65b1b343c2692d560)
