# Maximum lines per bookmark

In **Settings → Bookmarks and notes**, **Max lines per bookmark** is off by
default. The first enable offers four lines. You can choose 1–10. Entries grow
to fit their text up to the limit. Reader packs them into the available
height, with dates and locations below the title.

While the maximum is enabled, Reader uses the measured rows in place of the
fixed page density and automatic font shrink. Turning it off restores the
previous settings for both. The page count extends as Reader measures
subsequent pages. On a small viewport, an exceptionally tall note can use
the available page height and leave the rest of its text to the note view.

Reader measures each page using the same typography and row rectangles
that it draws and checks for taps and holds. Desktop tests cover a mixed
EPUB list, persistence and restoration of the previous fixed layout, and
an oversized PDF highlight on the second page.

Reader 0.1.31 passed 1,030 library tests, one Kindle binary test, 14 EPUB
package tests and four layout checks on the Mac. The native Oasis 3 suite
passed 1,034 library tests, 14 EPUB package tests and four layout checks.
Both skipped 62 optional tests. Eight workflow captures rendered pixel for
pixel across both platforms. The ninth capture shows the active system clock
and battery; all 818 changed pixels are confined to those indicators. The
book page, footer, and Exit match in that frame. Foreground physical input
and E Ink refresh remain separate checks.

[Flexible bookmark rows in the simulator](screenshots/flexible-bookmarks.png) · [KOReader maximum lines setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L95-L135) · [Reader workflow tests](../src/app/saved_marks_view/flexible/tests.rs)
