# Fixed-document reflow writing direction

## KOReader reference

At the pinned KOReader revision, `writing_direction` is part of the OCR/reflow
options. The choices are LTR, RTL and TBRTL, numeric values 0, 1 and 2, with
LTR as the default. KOReader enables the setting when text wrapping is on. Its
help says the choice controls the source text direction used for extraction
and reflow. [Pinned option definition](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L2900-L2932)

## Reader candidate

The fixed-document **Layout** panel exposes **LTR**, **RTL** and **TBRTL** for
embedded-text PDF reflow and OCR word-image reflow. Embedded-text TBRTL sets
`writing-mode: vertical-rl` on generated XHTML and uses Reader's existing
vertical text renderer. OCR TBRTL orders word crops from their source geometry,
places columns right-to-left, and continues an overlong column in the next
available column. LTR remains the default. Embedded-text and OCR direction
preferences are saved separately. Neither path changes the source page or OCR
word boxes.

Older Reader settings have no text-direction field. When present, the older
LTR/RTL preference seeds the new embedded-text setting, and otherwise it
resolves to LTR. Fixed-page defaults and compatible reading profiles carry
both settings. Existing saved settings that predate these fields remain
readable.

## Remaining difference and acceptance

The current OCR renderer reorders whole OCR word boxes from their geometry; it
does not perform Unicode bidirectional reordering within individual word
images. Bidi control characters are rejected because the word-image renderer
cannot safely apply their inline overrides. Embedded-text reflow applies a
base direction; finer bidirectional behavior depends on the text engine.
Vertical OCR column grouping, line breaks, mixed-script behavior, and source
anchor restoration still need comparison against KOReader on real scans.

This is an implementation candidate only. The feature has not yet passed
functional checks, simulator comparison, screenshots or Kindle acceptance.
Complete these checks in the deferred validation pass:

- Compare LTR, RTL and TBRTL paragraph and column order against KOReader for
  embedded text and OCR, one-column pages, multi-column pages, mixed-script
  lines and right-aligned text.
- Confirm direction is restored after leaving and reopening the book, and is
  carried by the fixed-document default and compatible reading profile.
- Compare scan word gaps and selection geometry after switching direction.
- Confirm vertical OCR text remains in reading order when a line crosses the
  viewport boundary and when detected page columns are enabled.
