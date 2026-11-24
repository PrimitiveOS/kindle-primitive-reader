# Reading footer text size and progress track

**Settings › Reading status bar › Item font size** accepts values from 8 to 36.
The default is 14, matching KOReader's item-font setting. Reader scales values
from 16 to 40 px while preserving 28 px at the default, so the largest setting
stays inside the compact Kindle footer. Saved sizes apply to both reflowable
books and fixed-page PDFs.

**Bold text** is a separate saved toggle, Off by default, following KOReader's
`footer.text_font_bold` option. It changes footer text weight without changing
the reading font. Reader measures the caption using the selected weight before
placing the adjacent progress track, so wider bold text still leaves the Exit
target clear. The pinned KOReader defaults and font menu use
[`text_font_bold`](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerfooter.lua#L525-L526)
and expose it beside the footer font controls
([font menu](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerfooter.lua#L1574-L1648)).

**Font face** can be selected independently of the book's reading font. Reader
offers TTF and OTF files copied to `reader/data/fonts`, with Noto Sans as the
device default. The saved selection is included in status-bar presets and is
used for both measuring and drawing the footer caption in EPUB and fixed-page
documents. The [face chooser](screenshots/footer-font-faces.png), [selected
setting](screenshots/footer-font-face-settings.png), [EPUB](screenshots/footer-font-face.png),
and [PDF](screenshots/footer-font-face-pdf.png) captures come from the simulator;
the host workflow test verifies selection, persistence, preset restore and both
document render paths.

The default footer uses the owner-requested two-row layout: `Chapter x of y -
Page m of n` on the first row, the horizontal book-progress bar and vertical
chapter ticks below it, and Exit on the far right. The track ends before the
Exit target. The chapter/page caption always uses a hyphen, even when a
different separator is selected for combined or custom status items. The
position, thickness, margins and chapter-tick width remain adjustable under
Reading status bar settings.

Desktop coverage verifies the 8–36 bounds, default, save and reopen behavior,
caption geometry, EPUB/PDF rendering, and the exact chapter/page format. A
regression confirms the chapter caption stays hyphen-separated when the saved
custom-item separator is a bullet. The simulator screenshots show the
[default two-row footer](screenshots/reading-footer-below.png), [large EPUB](screenshots/footer-font-large-epub.png)
and [PDF](screenshots/footer-font-large-fixed.png) text, plus the [regular](screenshots/footer-bold-off.png)
and [bold](screenshots/footer-bold-on.png) footer weights. The bold mode retains
the hyphen caption and keeps the progress track clear of Exit. Foreground E Ink
appearance on the Kindle remains open.
