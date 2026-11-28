# OCR scan reflow word gap

KOReader exposes Small, Auto and Large word-gap choices in its fixed-document
reflow controls. The pinned values are `0.05`, `-0.2` and `0.375`; the negative
value delegates spacing to the engine's automatic mode. The option is
described as a minimum spacing fraction of the lowercase “o” height in
k2pdfopt. [Pinned KOReader option](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L2540-L2557)

PrimitiveOS Reader reflows OCR word-image crops rather than k2pdfopt text.
Small and Large therefore use those stock ratios against the chosen scan text
height. Auto preserves the measured horizontal gap between adjacent OCR boxes,
scaled with the words; missing gaps use the median measured on that source
page, with a conservative fallback when all boxes touch. Gaps are bounded to avoid
large blank stretches. This is a native approximation of stock behavior, not
a claim of the same renderer or identical pixel output.

The Layout panel exposes all three choices. The setting persists per fixed
document, in fixed-document defaults, and through reading profiles. Older
profiles with no saved word-gap action retain the currently open book's value.
Changing the choice rebuilds the current reflow while retaining the visible
source OCR word as its anchor.

This implementation candidate has not entered behavioral validation yet.
During the planned validation phase, compare all three settings using real
Kindle documents with narrow, typical and expanded word spacing. Check RTL
text, columns, paragraph breaks, profile compatibility, cancel/apply behavior,
screen refresh and source-word anchoring on both simulator and Kindle.
