# Logical document font sizes

This source slice implements the pinned KOReader document size contract while
retaining the original physical-pixel mode. It is not a whole-interface Screen
DPI setting or a new footnote/dictionary font preference.

## Source contract

The pinned KOReader tree is `9192014d8bd82a91dc1012473be0f238dedfdb54` and
CREngine is `b32a88ffbbf505cf2ea5208c1a49002d70d3609f`.
`readerfont.lua:203–210`, `creoptions.lua:416–454`, and the number picker accept
finite logical sizes 12–255: eleven presets, half-unit taps, four-unit holds and
arbitrary typed decimals. Half steps do not quantize typed input.

`Screen:scaleBySize` uses the full screen's shorter edge divided by 600. With an
explicit Screen DPI override it averages this with override/160; ordinary
physical device DPI does not enter the Auto formula. The immutable scale context
is captured before facing-page width, footer overlap and content-DPI changes.
This slice uses Auto in the application; it does not expose a partial global
Screen DPI control.

`base/cre.cpp:1849–1862`, `lvdocview.cpp:3861–3882`, and
`lvrend.cpp:2398–2419` impose 12–340 px for the document base and 8–340 px for
computed fonts. On a 1264 px short edge, logical22 is47 px. Logical255 requests
538 px but renders340 px in pinned CRE. The same plateau is intentional here.
Author absolute font-size interpretation remains the existing limited parser;
this change does not establish a complete CRE CSS font cascade.

The committed vector fixture contains 7,380 outputs of the pinned Screen
function (15 screen/override contexts × 487 half steps and five typed decimals).
The effective-size column applies the separately inspected CRE clamp. It is
source-derived evidence, not 7,380 executed CRE renderings.

## Saved settings and application

`logical_font_size: null/missing` stays the legacy pixel mode. Missing values are
omitted on serialization, preserving old serialized preference shape and the
existing12–144 px validation,12–192 px shaping and small-mask paint path.
Existing raw pixel values remain stored while a logical choice is active.
Unrelated text adjustments and saved profiles retain that raw value rather than
mistaking a resolved47/340 px value for a logical size.

A new, unseen reflowable book without a saved default template explicitly starts
at logical22. Existing books and templates/profiles missing the field remain
legacy. Profiles, defaults, library writes and typed backup validation reject
nonfinite or out-of-range logical values before JSON can turn them into null.

The shared numeric editor prepares a draft, honors the saved logical default or
22, and restores the Type panel on Cancel. Draft/default changes do not convert a
legacy book. Apply resolves a candidate at the source anchor, validates the
edition before and after preparation, paints the candidate, writes state
atomically, and swaps the live page only after success. Font, layout or save
failure keeps the old page, position, raw preferences and draft.

## Large glyphs and geometry

Logical mode clamps actual computed text/list/image-em sizes coherently to
8–340 px, retaining the raw inheritance scale for nested percentages. Legacy
mode retains its existing clamps. Logical vertical columns reserve the largest
retained effective font rather than overlapping large runs.

The existing compact glyph cache remains bounded at2 MiB. A logical-mode glyph
which does not fit its300 px/64 KiB mask receives a visible256×256 tile fallback.
Contours are flattened with explicit segment/depth/coordinate bounds, clipped
as closed polygons before the small rasterizer sees them, and retain winding
holes. Only viewport-intersecting tiles are drawn. Signed stroke masks use the
same compositing contract; strength is bounded at54 px, covering the supported
340 px × weight1–1000 domain with26.6 rounding. Nonstandard font data outside
these bounds produces an explicit paint error rather than an empty valid page.
Large tiles are transient; they cannot grow the retained cache. Each clipped
scratch polygon is capped at131,072 vertices before pushing, and one glyph's
visible clipping shares a16,777,216-vertex visit budget. The256² rasterizer's
coverage scratch is262,160 bytes of f32 in addition to at most two64 KiB alpha
buffers; these are separate from the retained2 MiB cache. Stroke construction
first flattens to closed polylines and preflights a conservative
8×(vertices+contours)+8≤65,536 output bound before calling the pinned line-only
Miter builder. It cannot invoke curve-stroke subdivision or round caps.

A valid imported logical layout that cannot fit this viewport offers a visible
Reset text confirmation. Cancel leaves it intact. Reset prepares and paints a
known device layout at the original source anchor, atomically archives the old
choices under Reader data/layout-recovery, then saves before swapping the live
page. Archive or state-save failure keeps the old saved preferences and page.
It does not reapply a potentially unfit saved global template, and it does not
catch parser, DRM, missing-file or invalid-anchor failures as layout recovery.
The recovery archive contains only edition identity and typography preferences;
normal backup/profile/default validation preserves the active logical value.

Footnotes still use the existing compact18–64 px choice from the retained pixel baseline, independently of the logical body setting. Stock sends its
pre-CRE-clamp Screen-scaled request to a separate MuPDF popup and has independent
absolute/relative preferences; that is a separate remaining scope. Dictionary
text sizing is also independent.

## Verification status

Source and authored tests are prepared. Remote CI, Mac execution, full screenshot
inspection, original EPUB/AZW3/MOBI workflows, native compilation and physical
Kindle acceptance are pending. No local Cargo or device operation was run to
prepare this source slice. The tests cover the vector oracle, exact legacy
pixel/source behavior, large-font ink, effective size/selection, draft/cancel,
rollback, edition guards, profiles/defaults/backups, all487 live half-step sizes,
and eight compact/large/light/dark control captures. This is not yet release
acceptance.
