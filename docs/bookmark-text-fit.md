# Fit more text in bookmark rows

Under **Settings → Bookmarks and notes**, enable **Shrink bookmark font size
to fit more text**. Reader measures how the title or note text wraps in each
saved mark row and can reduce it by up to eight logical font units. It keeps
the selected base font size. Search and import actions retain their usual
typography. Dates and locations remain visible where there is room.

The desktop verification exercises a long EPUB note, saves the preference,
reopens the same list and follows the original source position. The fixed-page
test long-presses a highlight on page two of a dense list to verify that the
text adjustment leaves the row hit targets intact.

[Long note in the simulator](screenshots/bookmark-fit.png) · [KOReader setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L187-L197) · [Reader tests](../src/app/saved_marks_view/tests.rs)
