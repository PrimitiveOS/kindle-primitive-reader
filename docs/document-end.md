# End-of-book actions

Advance once from the last page or the bottom of a continuous document. Reader opens a compact panel over the page with actions for reading status, restarting, opening the next file in the folder, deleting the file and returning to Library. Dismiss with **Close**, a tap outside the panel or the physical back button. The visible page and saved position stay where they were.

<img src="screenshots/document-end.png" width="390" alt="Reader’s six end-of-book actions over the final page of an authored EPUB">

Go to **Reading controls › At the end of a book** to select the default action. **Always mark as finished** can be independent of that choice. **Open next file** walks the folder in the Reader file picker’s order and skips unsupported entries. **Go to beginning** records the last page as a return location when the move changes the page or location. The Library action brings you back to the book’s folder.

**Delete file** presents a separate confirmation. It deletes the source file and keeps Reader’s saved notes, bookmarks and reading position. Reopening the same edition after restoring the file brings those records back. Whole unpacked EPUB folders can be managed in Files.

## Reference and tests

The installed KOReader source matched the v2026.07.1 reference. Thirteen checks exercised its dialog, automatic and manual reading status, next-file access under different sort orders, return to the beginning, Library, and the independent confirmation before deletion. Reader uses its natural filename order across regular document folders. Both reading providers treat a forward attempt at the boundary as the trigger. [KOReader end-of-book actions](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerstatus.lua#L38-L222) · [Reflow page turns](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerrolling.lua#L911-L945) · [Fixed-page and continuous turns](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerpaging.lua#L912-L1047)

The Mac checks exercised all nine action preferences and the separate auto-finish setting, along with the end of a comic spread, confirmation and cancellation, reopening with saved marks, and the previous location after restarting. Four original reflowable books and three original PDFs each passed the same paged and continuous boundary checks. Deletion tests used a disposable authored copy.

The same seven originals passed the boundary checks on the Oasis 3 from its Documents folder. Their SHA-256 checksums matched after the run. The end-of-book panel and the deletion confirmation from the authored test produced byte-identical screenshots on the Mac and Oasis. The Kindle run used temporary Reader state with a separate disposable copy for the deletion check.
