# Bookmark browser

Open **Library › Bookmark browser** in the upper reading bar, or **Organize › Bookmark browser** in the document browser. Choose History, collections, selected books, archived records, Documents, or a folder. Documents and folder sources offer a choice to include subfolders. Books without saved marks stay out of the results.

<img src="screenshots/bookmark-browser.png" width="390" alt="Saved notes and page bookmarks grouped by author and book with Sources, Filters and Find controls">

Results group saved marks under each book's author and title. **Filters** selects page bookmarks, highlights, notes or a drawing style. Select individual books, select all, or replace the selection with an author's books. **Find** searches the saved quotation and note with Unicode case folding. Turn on **Match case** for case-sensitive matching. A match stays inside the quotation or note.

Reading order follows source positions. **Date added** sorts each book's marks by their creation time. **Sort direction** reverses that order. Earlier marks without a recorded creation time appear after dated marks. The original reading order resolves equal or missing dates. The type, style and sorting preferences persist between sessions. Search text and the temporary book filter clear when the browser opens again.

Tap a mark to preview its full quotation and note. **First**, **Previous mark**, **Next mark** and **Last** move between matching marks and skip book headings. The physical page buttons move between marks. Long text has its own text controls and vertical scrolling. **Back** returns to the result page containing the selected mark.

<img src="screenshots/bookmark-details.png" width="390" alt="A saved PDF note with a View in book action and controls for moving between matching notes">

**View in book** opens the source edition and jumps to the saved page or anchored text. Reader retains the target book's previous location in its navigation history. A protected PDF asks for its password before completing the jump. Cancel returns to the mark preview. Archived records remain readable while their source file is unavailable.

Hold a direct source row to save it as the default. For a collection or book selection, hold **Apply collections** or **Apply book selection**. For a chosen folder, hold **Use this folder**. The source chooser includes a visible control to clear the saved default.

## KOReader comparison

The comparison uses the installed KOReader v2026.07.1 bookmark browser and its source. Its source chooser, grouped results, type and style filters, literal search, date sorting, preview navigation, archived records and jump history provide the reference behavior. The test harness loads KOReader's actual data and preview methods with presentation dependencies replaced in memory. It performs 20 checks without displaying a KOReader screen or changing its saved data. [KOReader browser](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/ui/widget/bookmarkbrowser.lua) · [Comparison harness](../scripts/koreader-bookmark-browser.lua)

The simulator tests drive the normal touch and physical-button inputs through sources, collection selection, author selection, search, previews, PDF authentication and location restoration. Four authorized original Documents files, two EPUBs and two PDFs, also restore saved marks and the previous page while retaining their source bytes. The full Mac suite passes 558 library tests, 14 integration tests and four system tests. Kindle verification for this increment is pending.

The remaining comparison includes per-mark color filtering, series, language and keyword selection, richer book information actions, and asynchronous loading for large marked libraries. These remain open in the full reading-workflow audit.
