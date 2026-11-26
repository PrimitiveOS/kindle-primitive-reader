# Local annotation exchange

While reading, open **Settings › Bookmarks and notes › Annotation exchange**.
**Export annotations on book closing** and **Keep all annotations on import**
are independent preferences and both default to Off. Use **Export / import
folder** to select an existing folder in Documents, or return to the book’s
metadata folder. The default file is `<book stem>.sdr/<book filename>.annotations.lua`.
Transfer files contain bookmarks, highlight locations, styles, colors, notes,
and available dates. This workflow uses KOReader’s built-in book transfer.
The separate **Export annotations** screen still produces JSON, Markdown and
other reading exports.

Reader also implements a page-bookmark note candidate. In the open book’s
saved marks list, hold a page bookmark to enter or edit its note; save an
empty note to remove it. Notes are saved separately from highlight notes,
searched with Notes enabled, shown in the saved-mark list and library-wide
bookmark browser, and included in backup and KOReader annotation transfers.
Older bookmark records without a note remain readable. Page-bookmark titles
remain location labels, not extracted page passages. The open-book export
includes these bookmarks and notes in plain text, Markdown, HTML and Kindle
clippings. The first three identify bookmark labels separately from quoted
highlights; clippings use a bookmark record and a separate note record at the
same location. Fixed-page clippings identify the page, while reflow clippings
identify the chapter because displayed pages can change with layout. Library-wide
exports also include saved page bookmarks and notes when both highlight filters
are clear. Bookmark-note editing, legacy imports,
transfer compatibility, search, persistence, and these export additions remain
unverified on the computer and Kindle.

Turn on close export to write the transfer after Reader saves and closes the
book, exits, or finishes preparing a different book. Sleep and cancelled or
failed document opens keep the current book open. **Export annotation
transfer** writes immediately. A complete file can replace Reader’s own
previous snapshot for the same edition or a foreign snapshot already applied
in full. Unresolved source marks or a pending foreign transfer leave the
existing file intact. Source books remain read-only.

Use **Import annotation transfer** to preview the number of marks it will add,
update or remove. A matching newer mark replaces its older local record.
With **Keep all** on, older marks missing from the transfer remain. With it
off, those older marks can be removed after explicit confirmation. Reader
preserves unmatched marks when the transfer cannot verify its edition or
one or more of its locations. A stock transfer without a fingerprint can
remove older marks only when it is in the book’s adjacent metadata folder.
Dated imported marks compare with the saved
local edit time when available. An undated local mark stays. Reader remembers
the exact bytes of successfully applied complete files, so repeating an
import keeps one copy of each matched mark.

Reader verifies EPUB source pointers and text before applying them. A fixed
source page is one-based on the wire and selected text is checked against
its page rectangles. The current adapter covers packaged reflowable EPUBs
and supported PDF and DjVu locations. A multi-page fixed highlight uses
KOReader's page-indexed `ext` rectangles: Reader verifies the page keys,
positions, document bounds and selected words before merging it. An unsupported
or mismatched entry is counted as unavailable and prevents removal of older
local marks. Reader keeps page geometry, style, color and notes; it inserts its
own newline between imported page excerpts. Multi-page transfer is an
unverified implementation candidate, not yet compared on desktop or Kindle.
Unsupported structural XPointers remain open. On opening a supported EPUB, PDF or DjVu, Reader now checks the configured transfer folder, or the default adjacent `.sdr` folder, on a bounded, cancellable background worker. A valid, different-device transfer whose exact bytes have not already been applied produces an advisory toast and a **Navigation → Review annotation transfer** action. This is only a candidate: choosing it runs the existing preview, which rechecks the book and transfer before offering any explicit import. The existing preview can still perform synchronous source and mark verification. Discovery reads at most 2 MiB, does not interrupt the transfer decoder while it is parsing, skips symlinked paths, and drops stale or cancelled results. For a shared configured folder, it requires a transfer fingerprint matching the book’s bounded KOReader sampled fingerprint to avoid suggesting another edition with the same filename; a transfer without a fingerprint remains available through manual Import preview. The source fingerprint samples at most 12 KiB and cannot interrupt an individual file read. Sleep, book changes and USB storage return cancel the scan; it restarts when appropriate. Reader never deletes the transfer as part of discovery or review. Automatic merging and consuming the file remain open; KOReader checks foreign transfers on open and removes the consumed file. KOReader also supports
other global metadata folder policies. [Stock source for export paths, dates and merge](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerannotation.lua#L250-L333) · [Stock settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L263-L301)

## Mac comparison

The pinned KOReader Lua module exercises default and chosen paths, close
export disabled and enabled, the same-writer check, both Keep all policies,
the newer record’s edit date and transfer consumption. The native replay loaded Reader’s authored EPUB and PDF transfers through
the stock Lua settings reader. CREngine resolved the bookmark and both
selection endpoints, and returned the exact quotation. MuPDF selected the
exact PDF word from the exported positions. The two PDF rectangles agree
horizontally and overlap vertically across their different font metrics. [Stock replay](../scripts/koreader-annotation-exchange-check.lua)

Four Mac simulator tests exercise toggles, folder selection, Library close
and Exit, sleep, failed switches, resuming the same position, the import
preview, safe rollback on a storage failure, repeated imports and fixed-page
geometry.
[Annotation exchange settings](screenshots/annotation-exchange-settings.png) ·
[Import preview](screenshots/annotation-exchange-preview.png)
