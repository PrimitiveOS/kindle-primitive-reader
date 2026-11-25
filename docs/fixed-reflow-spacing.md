# Line spacing in fixed-document reflow

Open **Scan → Read text** or **Scan → Reflow scan**, then tap **Layout** in
the reading footer. Choose **Small**, **Medium**, or **Large** and tap
**Apply**. **Cancel** and the back page button close the panel and restore the
previous view. Reader saves the choice for that document and uses it in both
reflow views. Medium retains the existing layout for books with no saved choice.

The reflow view keeps the current source passage when changing spacing.
Reader uses a source text anchor for Read text and a word ordinal on the
original source page for Reflow scan. It retains the anchor’s position
within the viewport. When starting in Medium, undoing the adjustment
without a page turn restores the previous viewport. The underlying PDF or
DjVu page, annotation coordinates, and fixed-view position stay in place.

The pinned KOReader MuPDF and k2pdfopt renderers processed authored
vector-text and raster-only versions of the same PDF. The control uses
relative line pitch factors 1.0, 1.2, and 1.4 on supported reflowed text.
These are the first two line origins measured in the desktop simulation.

| Source | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| Embedded text PDF | 51 px | 61 px | 72 px |
| Raster-only PDF | 52 px | 62 px | 73 px |

Line and word counts and first-line ink dimensions stayed the same across
those three KOReader settings on both authored first pages. The probe also
measures the other stock controls. A smaller word gap reduced wrapping at
1.6×. The font scale changed ink size and wrapping. Full justification
changed extracted word boxes. Auto, Left, Center, and Right produced the
same measured boxes on the left-set specimen.
[Executable comparison](../scripts/verify-stock-scan-reflow-controls.lua) ·
[KOReader options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L376-L482)

Reader applies the stock ratios around its existing Medium pitch. An
authored Reader unit fixture measured 44, 53, and 62 pixels with the same word
sizes and order at each setting. Read text
uses its existing layout engine and the source text layer, with cached OCR when
needed. Reflow scan wraps original word images from saved OCR boxes and
keeps its horizontal left-to-right limits. Long expressions may use different
line breaks, and Reader can place the source anchor partway down the new view
page. This setting covers line spacing. The stock provider also uses its own
word-gap, alignment, font-scale, image-handling, and page-segmentation logic.
[Fixture generator](../scripts/generate-scan-reflow-fixtures.py) ·
[Spacing controls on an authored scan](screenshots/fixed-reflow-spacing.png)
