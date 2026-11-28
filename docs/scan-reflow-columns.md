# OCR scan reflow column control

## Reference behavior

The pinned KOReader `koptoptions.lua` exposes **Document Columns** with
values 1, 2, and 3 when document reflow is active. Its help explains that the
setting caps column detection and that one column can avoid false splits caused
by unusual word spacing. KOReader's `defaults.lua` sets the default to two.
[Pinned option](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L3007-L3035) ·
[Upstream default](https://github.com/koreader/koreader/blob/master/defaults.lua)

## Reader behavior implemented

In OCR **Reflow scan › Layout**, the user can select 1, 2, or 3 columns. The
choice defaults to two, is saved per fixed-page book, and is included in fixed
document defaults and reading profiles. The scan renderer uses bounded raster
gutter detection; when it finds a permitted two- or three-column layout, it
groups OCR word images by detected column and line, then follows the selected
LTR, RTL or TBRTL writing direction. TBRTL groups vertical OCR word boxes by
their x positions, orders vertical lines from right to left, and words from top
to bottom. LTR is the default. A setting of one disables page-level gutter
detection. If a clear split is not found, the renderer keeps the OCR provider's
page-column grouping and uses the selected writing direction within it.

The reflow anchor identifies the original OCR word rather than its temporary
position in the reflowed line. Changing the column limit therefore keeps the
same source passage in view. The existing temporary **Tools › Columns** crop
view remains a separate feature.

The same **Layout** panel exposes **Word gap** choices Small, Auto and Large.
Auto scales adjacent OCR-box gaps with the scan text size and falls back to a
median measured gap when an individual pair has no geometric separation.
Small and Large use the pinned KOReader preset ratios. This adapts the stock
setting to a word-image reflow engine rather than claiming pixel-identical
k2pdfopt behavior. The selected value is saved per book, in fixed defaults,
and in reading profiles.

The same **Layout** panel exposes **Alignment** choices Auto, Left, Center,
Right and Justify for horizontal writing. Auto estimates dominant source-line
alignment from OCR geometry and falls back to the selected LTR or RTL writing
direction if the page is ambiguous. TBRTL uses top alignment and hides the
horizontal alignment choices. Justified reflow distributes extra width across
word gaps, leaving paragraph-final lines ragged. This behavior is an unverified
native candidate.

This is an unverified implementation candidate. The user's requested
implementation-first sequence defers behavioral tests and all simulator and
Kindle acceptance until the implementation pass is complete.

## Acceptance checklist for the deferred validation pass

- [ ] Compare the 1, 2, and 3 column controls, default, help and saved value to
  the pinned KOReader setting.
- [ ] Exercise single-column pages, two-column pages, three-column pages,
  unbalanced columns, full-width headings, faint gutters and pages without a
  confident split. Confirm forced one-column mode does not report a false
  multi-column ordering failure.
- [ ] Verify LTR, RTL and TBRTL column and line order with known OCR text; check
  that vertical lines crossing the viewport and paragraph breaks remain
  sensible.
- [ ] Change the maximum while reading below the first screen, then test
  Cancel, Apply, reopen, orientation change and page turn. Confirm the same
  recognized source word stays in view.
- [ ] Verify per-book persistence, new-document defaults and profile
  round-trips without changing existing crop, zoom, page, bookmark or
  annotation coordinates.
- [ ] Compare Small, Auto and Large against KOReader's reflow output for text
  with tight, typical and unusually wide OCR word boxes; reopen to check the
  saved value and source-word anchor.
- [ ] Capture the compact Layout panel and reflow output in the Mac simulator;
  compare source reading order and first visible passage to the same pages in
  KOReader.
- [ ] On the Oasis, check touch targets, refresh behavior, OCR latency, sleep
  and resume, USB resume, memory, and reopen on real multi-column documents.
