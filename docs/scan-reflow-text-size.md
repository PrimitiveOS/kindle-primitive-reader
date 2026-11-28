# OCR scan reflow text size

## KOReader reference

At the pinned KOReader revision, fixed-document reflow offers scale presets
`0.2, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.3, 1.6, 2.0`. It also exposes
fine adjustments of `−0.05` and `+0.05`; the preset default is `1.0`, and the
fine-adjustment default is `+0.05`. KOReader describes the factor as scaling
the source document's original font sizes. [Pinned scale controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L1678-L1684) ·
[Pinned fine adjustment and help](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L2465-L2529) ·
[Pinned default values](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/defaults.lua#L684-L716)

## Reader candidate

Reader saves the effective scale in percent, from 20 through 200 in steps of
5. The initial value is 105, matching KOReader's 1.0 preset plus its default
+0.05 fine adjustment. The footer shows the effective value as a multiplier.
The minus and plus controls each move by 0.05, so every pinned preset and
fine-adjustment value in range can be selected without storing a second
adjustment.

For a scanned page, Reader measures the median height of its OCR word boxes,
projects that height from source-raster width into the reading column, applies
the saved factor, and repositions the original word-image crops at the
resulting scale. The target is clamped to 6–128 pixels before layout. Word
gaps, line spacing and paragraph spacing use the resulting target height.
This keeps the factor relative to the source page's OCR geometry when scans
have different resolutions. It is not the same as KOReader's k2pdfopt font
renderer, and raster output will differ.

The scale is saved per fixed-page document, copied to fixed document defaults
and reading profiles, and migrated from the earlier unpublished pixel-height
field. A missing scale in an older profile remains absent so applying that
profile does not overwrite the current book's value.

## Deferred comparison

The feature remains unverified. When implementation is complete, compare the
stock preset list, 1.05× default, fine adjustment, visible value and saved
behavior. Use low- and high-resolution scans, varied OCR box heights, short
words, headings, multiple columns, paragraph ends and RTL pages. Check the
6–128 px target clamp, line breaks, word gaps, source-word anchors, profile and
document-default migration, Cancel/Apply, reopen and E Ink refresh on the Mac
simulator and Oasis.
