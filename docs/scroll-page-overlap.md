# Continuous-scroll page overlap

KOReader's continuous reader can repeat the last lines after a page turn so
the reader can follow a sentence across screens. The pinned implementation
uses a disabled-by-default switch, one repeated line by default, a range of one
to ten lines, and five indicators: none, dim, arrow, solid line and dashed
line. [Settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/page_overlap.lua)
· [Scroll page-turn behavior](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua)

Reader exposes the same switch, line range and indicator choices in the Text
panel while continuous scrolling is active. Choices are saved per book and can
also be included in the default text profile for new books. Older profiles
retain the off, one-line default. Page-button and automatic turns subtract the
configured repeated-line height from their movement. Direct swipes remain
unadjusted panning gestures. The default height follows the current font size
and line spacing. As in KOReader, the marker appears for a full-page turn, not
for fractional automatic scrolling, and is suppressed near the end of a
chapter.

The focused desktop regressions verify forward and reverse automatic turns,
page-button movement, unchanged swipe distance, all five indicator styles,
and persistence across old and new per-book settings. The simulated grayscale
frame shows the repeated area with KOReader's default gray cue:

![Reader continuous-scroll page overlap in the desktop simulation](screenshots/scroll-page-overlap.png)

The Text → Layout panel exposes the per-book controls and the 1–10 line
choices:

![Reader page overlap controls in the desktop simulation](screenshots/scroll-page-overlap-settings.png)

The Kindle comparison remains open. In particular, verify line placement and
text legibility on the installed Oasis 3 build and check gray-area refresh
artifacts on the actual E Ink panel before marking the controls verified.
