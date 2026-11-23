# Reading footer text size and progress track

**Settings › Reading status bar › Item font size** accepts values from 8 to 36.
The default is 14, matching KOReader's item-font setting. Reader scales values
from 16 to 40 px while preserving 28 px at the default, so the largest setting
stays inside the compact Kindle footer. Saved sizes apply to both reflowable
books and fixed-page PDFs.

The default footer uses the owner-requested two-row layout: `Chapter x of y -
Page m of n` on the first row, the horizontal book-progress bar and vertical
chapter ticks below it, and Exit on the far right. The track ends before the
Exit target. The position, thickness, margins and chapter-tick width remain
adjustable under Reading status bar settings.

Desktop coverage verifies the 8–36 bounds, default, save and reopen behavior,
caption geometry, EPUB/PDF rendering, and the exact chapter/page format. The
simulator screenshots show the [default two-row footer](screenshots/reading-footer-below.png)
and [large EPUB](screenshots/footer-font-large-epub.png) and
[PDF](screenshots/footer-font-large-fixed.png) text. A test-only ARMHF run on
the Oasis passed 954 Reader tests, 14 EPUB package tests and four layout probes.
That run did not replace the installed app or verify foreground visual sizing.
Font-face and bold choices remain open.
