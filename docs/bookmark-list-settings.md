# Bookmark list settings

Open the upper reading bar and choose **Settings → Bookmarks and notes**.
**Rows per page** accepts 6–24 with a default of 14. **Bookmark font size**
accepts 10–72 logical units. In Automatic, the size follows the row count,
starting at 19 at 14 rows. The rendered title is capped to fit its row.

Reader uses these settings for the in-book EPUB and fixed-page bookmark lists
and the in-book highlights list. Existing search, import, and export actions
keep the standard list typography and count toward the first page's row
limit. Existing sort, dates, notes, source locations, and the cross-book
bookmark browser keep their behavior.

The desktop workflow changes the row count and size, sorts by date, reopens
the preference store, and uses a long press on a highlight on the second page
of a fixed-layout list. The workflow verifies that the correct saved mark
opens from the smaller row geometry.

[Bookmark list screenshot](screenshots/bookmark-density.png) · [Reader workflow](../src/app/saved_marks_view/tests.rs) · [KOReader row and font controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L130-L185)
