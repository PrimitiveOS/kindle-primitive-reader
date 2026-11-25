# Two-column reading on dark scans

Reader's two-column scan view can detect light text on dark source pages as
well as dark text on light paper. The detector samples both dark and light
strokes before choosing the page polarity. It returns the same source-page
coordinates to the existing column navigation; it does not invert or modify the
book image. Blank pages, unclear layouts and ambiguous multiple gutters still
fall back to the normal page view.

The KOReader reference exposes document inversion for image-heavy books and
uses its k2 fixed-page pipeline. Reader's detector is independently implemented;
polarity and page geometry have not yet been compared with KOReader. Source
checks were added for dark paper at several contrasts, ambiguous layouts and
scaled pages, but they have not yet been run. Simulator and Kindle validation
are also pending. [KOReader document inversion](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L691-L700) · [Reader detector and tests](../src/scan_columns.rs)
