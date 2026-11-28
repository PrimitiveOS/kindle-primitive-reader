# Zoom by columns or rows

On a fixed-page document, open **View → Zoom to**, then choose **Columns** or
**Rows**. Choose one to eight sections, or select **Custom** to enter a
count from 0.1 to 8.0 in tenths. Column and row counts are kept separately;
tile zoom is unavailable in two-page spread mode. Reader starts at the edge specified by
the book’s reading direction. Page buttons and page-turn taps move
through overlapping sections in the saved axis order before advancing
to the next page.

With 40% overlap, two columns occupy 1.6 screen widths. Three rows at
the same overlap occupy 2.2 screen heights. Reader uses KOReader’s
width-relative factor across pages of different sizes. The default
status bar shows the selected count, and Reader saves it with the
document. Saved profiles retain the tile axis and count for PDFs, DjVu,
comics, fixed-layout EPUB and images when the view has no facing pages. Applying
a tile profile to a book with facing pages still enabled is rejected without
changing the book's saved or visible view.
Selecting a fit option or a percentage leaves column or row zoom. A temporary
full-page search view restores the saved zoom when dismissed.

The authored acceptance case covers scale calculation, forward and backward
traversal, rotated rows, overlap changes, transitions among all five zoom
targets, independent custom counts, and reopening the same viewport after a
restart. It has not been run in this implementation pass. [Existing simulator capture](screenshots/fixed-tile-zoom.png) · [KOReader count options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L220-L249) · [KOReader zoom calculation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerzooming.lua#L654-L680)
