# End-of-book actions

Advance once from the last page or the bottom of a continuous document. Reader opens a compact panel over the page with actions for reading status, restarting, opening the next file in the folder, deleting the file and returning to Library. Dismiss with **Close**, a tap outside the panel or the physical back button. The visible page and saved position stay where they were.

<img src="screenshots/document-end.png" width="390" alt="Reader’s six end-of-book actions over the final page of an authored EPUB">

Go to **Reading controls › At the end of a book** to select the default action. **Always mark as finished** can be independent of that choice. **Open next file** walks the folder in the Reader file picker’s order and skips unsupported entries. **Go to beginning** records the last page as a return location when the move changes the page or location. The Library action brings you back to the book’s folder.

**Delete file** presents a separate confirmation. It deletes the source file and keeps Reader’s saved notes, bookmarks and reading position. Reopening the same edition after restoring the file brings those records back. Whole unpacked EPUB folders can be managed in Files.

## Return to the current book folder

While reading, open the **Library** menu and choose **Current book folder**.
Reader saves your position and marks, then shows the containing folder on
the page where the book appears. This also works when you opened the book
from Continue reading, search or a direct document link. An extracted EPUB
appears as its containing package folder in the parent list.

KOReader’s Home action closes the reader and passes the current file to its
file chooser so the directory opens at that file. The Reader shortcut
uses the same sequence after checking the destination can be read.
[Pinned KOReader Home action](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L929-L934) · [Pinned file chooser handoff](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L581-L595)

Desktop tests cover a resumed EPUB beyond the first folder page, saved
bookmarks and position after restart, fixed PDF, extracted EPUB, and
failed folder lookup or save. [Reading menu](screenshots/current-book-folder-menu.png) · [Containing folder](screenshots/current-book-folder.png)

## Switch between files in a folder

From the reading **Library** menu, choose **Previous file in folder** or
**Next file in folder**. Reader looks for the next unfinished publication
in that direction using the file picker’s natural order. It skips unsupported
entries and stops at the folder boundary. Reader saves the outgoing
position and marks before opening the selected document. The selected
document resumes at its saved position. If you cancel a PDF password,
Reader returns to the open book.

KOReader exposes both directions as reading actions and searches its active
folder order for supported files that are still unfinished. Reader uses the
same filtering for its manual actions. **Open previous document** uses
the recently read list. [Pinned KOReader actions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L76-L79) · [Pinned folder chooser](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/filechooser.lua#L425-L450)

Desktop tests check both directions across a saved EPUB and PDF, both
folder boundaries, skipping completed and unsupported files, an extracted
EPUB, and wrong-password retry and cancellation. [Reading menu](screenshots/adjacent-folder-files-menu.png) · [Resumed EPUB](screenshots/adjacent-folder-files-target.png)

## Reference and tests

The installed KOReader source matched the v2026.07.1 reference. Thirteen checks exercised its dialog, automatic and manual reading status, next-file access under different sort orders, return to the beginning, Library, and the independent confirmation before deletion. Reader uses its natural filename order across regular document folders. Both reading providers treat a forward attempt at the boundary as the trigger. [KOReader end-of-book actions](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerstatus.lua#L38-L222) · [Reflow page turns](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerrolling.lua#L911-L945) · [Fixed-page and continuous turns](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerpaging.lua#L912-L1047)

The Mac checks exercised all nine action preferences and the separate auto-finish setting, along with the end of a comic spread, confirmation and cancellation, reopening with saved marks, and the previous location after restarting. Four original reflowable books and three original PDFs each passed the same paged and continuous boundary checks. Deletion tests used a disposable authored copy.

The same seven originals passed the boundary checks on the Oasis 3 from its Documents folder. Their SHA-256 checksums matched after the run. The end-of-book panel and the deletion confirmation from the authored test produced byte-identical screenshots on the Mac and Oasis. The Kindle run used temporary Reader state with a separate disposable copy for the deletion check.
