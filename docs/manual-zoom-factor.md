# Fixed-page Manual zoom

KOReader's fixed-page zoom target includes Page, Content, Columns, Rows and
Manual. Manual mode uses the detected used-content bounds, fits their width,
and multiplies that fit by the saved zoom factor (default 1.5×). Its factor
presets range from 0.7× to 20×, with 0.1× fine adjustment.
[Pinned controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L220-L298) ·
[Zoom calculation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerzooming.lua#L524-L573)

## Reader candidate

Reader's fixed-page View panel exposes all five KOReader zoom targets:
Page, Content, Columns, Rows and Manual. Columns and Rows each retain their own
custom count from 0.1 to 8.0, even while another target is active. Manual mode
trims to detected content, fits its width, and applies a factor that defaults
to 1.5×. Its controls show the eight stock presets and a three-button fine
adjustment for −0.1×, the current value, and +0.1×. The range is bounded to
0.1×–20×. The fit axis (Full, Width or Height) is retained independently and
restored when switching between Page, Content, tile zoom and Manual. Counts,
axis and factor persist with book state, fixed-document defaults and reading
profiles. Existing zoom-index saves remain readable, and older in-progress
factor saves without an explicit target are interpreted as Manual.

The factor is active only in Manual mode. Switching to another target retains
the saved Manual factor for a later return. The five targets share the retained
fit axis; Columns and Rows also retain independent custom counts. Selecting
Page from a tile target exits tile mode and restores the axis. Manual mode uses
Reader's bounded content-box detection and existing source-coordinate
transform. Provider behavior, zoom anchoring, interaction with crop and
rotation, large-factor memory limits, and E Ink refresh cost remain unverified.

## Deferred acceptance

- [ ] Compare Page, Content, Columns, Rows and Manual target behavior, visible controls,
  selected states, factor presets, fine-step direction, and limits against
  KOReader on the same fixed-page documents.
- [ ] Verify Manual target and custom values, including 0.1×, 0.7×, 1.5×,
  10× and 20×, across close/reopen, defaults, profiles, all five targets,
  fit axes, independent custom column/row counts, rotation, crop and continuous
  reading.
- [ ] Check visible-page anchoring and pan when changing factor, repeated
  physical button adjustments, E Ink refresh cost, and bounded memory at the
  upper limit.
- [ ] Capture simulator comparisons and run foreground Kindle acceptance after
  the implementation pass is complete.
