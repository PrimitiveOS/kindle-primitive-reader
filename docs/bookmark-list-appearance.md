# Bookmark separators and highlight cues

Open the upper reading bar and choose **Settings → Bookmarks and notes**.
**Show separator between items** adds thin dividers to fixed-height and
flexible bookmark lists. New profiles start with dividers off. Older saved
preferences without this setting keep their existing dividers.

**Show highlight colors** draws a grayscale swatch and the color name on
highlight and note rows. **Also show default highlight color** includes
marks that use the current book's default color. A saved mark with an
explicit older Gray value still appears as Gray.

The two list modes keep the same row bounds when changing these settings.
The workflow tests exercise EPUB and PDF notes and highlights, compare the
before and after pixels, and open the same item through a long press.

[Before](screenshots/bookmarks-appearance-before.png) · [After](screenshots/bookmarks-appearance-after.png) · [KOReader list appearance settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L210-L250) · [Reader workflow tests](../src/app/saved_marks_view/flexible/appearance_tests.rs)
