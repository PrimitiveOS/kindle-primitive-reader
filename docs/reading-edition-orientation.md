# Screen orientation for each edition

In the lower Screen panel, tap Follow Launcher, Upright or Upside down to set the open edition’s screen orientation. Quick Display uses the same setting. Hold one of the three choices and confirm Set as default to change the starting orientation for either new reflowable books or new fixed-page documents. An edition opened before a default change keeps its saved choice. The existing app orientation supplies the fallback when a format has no saved default. [Reader Screen controls](../src/app/bottom_config.rs) · [Reader orientation policy](../src/app/reading_orientation.rs)

Turn on Keep orientation to carry the current choice through the Library, other books and a Reader restart. Visiting another book leaves its saved choice intact. A deliberate orientation change in an open book saves that edition’s choice and updates the held orientation. Turning Keep orientation off leaves the current reading view in place. The next book or Library view uses its own setting. Follow Launcher remains an inherited policy during sticky use, and older Launchers keep control of the screen while Reader retains its saved preference.

| Lower Screen controls | Hold a choice to set the format default |
| --- | --- |
| ![Screen orientation choices and Keep orientation beneath an EPUB page](screenshots/edition-orientation-panel.png) | ![Default book orientation confirmation over the same page and controls](screenshots/edition-orientation-default.png) |

## Pinned KOReader comparison

With sticky rotation off, KOReader reads the current edition’s rotation first, then the appropriate reflow or fixed-page default, and finally upright. Its File Manager reads its own default. Sticky rotation skips those screen resets and also skips copying the live screen turn to the book on save. An explicit bottom-bar selection still changes the book’s configurable value. Holding a choice writes the format default after confirmation. KOReader stores the last screen turn at app exit. Reader keeps the existing Follow Launcher option and preserves a held portrait choice across restarts. [Edition load and live rotation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L958-L975) · [Edition capture](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L1209-L1222) · [Sticky switch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/screen_rotation_menu_table.lua#L64-L76) · [File Manager](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L63-L69) · [Explicit option dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/configdialog.lua#L980-L1018) · [Configured edition save](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/configurable.lua#L45-L65) · [Format default](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/configdialog.lua#L1392-L1426)

The Mac comparison executed the pinned Lua callbacks for both document types and the File Manager. Reader tests opened authored EPUB and PDF files, switched between editions, changed and cancelled format defaults, and reopened private state. The original EPUB source anchor, PDF page and independent PDF page rotation survived those actions. The simulated screenshots show the actual lower Screen panel and the confirmation sheet.

## Landscape session preparation

`Reading::prepare_viewport` builds a separate reflow session and its rendered
frame for the requested canvas. The layout, raster stride, footer reservation
and selection coordinates use that session's geometry. It passes the same
canonical chapter, block and byte offset to the new pagination and checks that
the anchor is visible before accepting the candidate. The current frame and
queued page turns survive a refusal, as does the old geometry's page-count
cache.

The staged transition covers single-page, facing-page and continuous
horizontal reflow at 1264×1680 and 1680×1264. A spread uses two 616-pixel
leaves in portrait and two 824-pixel leaves in landscape, separated by the
same 32-pixel gutter. Reader lays out each leaf with its own margins and
uses those same physical origins for words, links, annotations and note
markers. EPUB spine direction controls which side contains the first leaf.
Page turns still advance two logical leaves. The final unpaired leaf keeps
its normal side and the empty pane stays blank.

Vertical writing uses the full current viewport and keeps the saved facing
preference for later horizontal chapters. Resizing while a vertical
chapter is active remains deferred until its writing-axis placement can
preserve the source anchor across a size change.

### Pinned KOReader facing-page behavior

CREngine has optional checks for minimum usable width and a 6:5 aspect
ratio. KOReader explicitly disables those checks when setting its visible
page count, so both portrait and landscape can use two leaves. Scroll mode
always uses one. During a dimension change the rolling reader enables native
history, resizes and updates the source pointer before drawing again. The
Mac comparison ran the pinned wrapper and native CREngine against *Field
Notes* and the right-to-left *Harbor notebook*. Both kept two leaves on
resize, one while scrolling and the same source pointer on return to portrait.
[KOReader visible-page wrapper](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1382-L1389) · [CREngine pane-count policy](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L3760-L3789) · [KOReader resize callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L1098-L1118) · [Mac comparison script](../scripts/koreader-facing-viewport-check.lua)

The app runtime still has to prepare its chrome and canvas, publish an
accepted frame through the shared handoff, and change the input generation
together with the reading session. Its lists, overlays and hardware resume
need to use those committed dimensions. The landscape control stays disabled
until those paths are integrated. [KOReader dimension-change path](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L892-L919)

Run the authored EPUB transition tests with
`cargo test --locked --lib session::viewport_tests`. Set
`READER_REFLOW_VIEWPORT_CAPTURE_DIR` to save the portrait, landscape and
returned frames. The fixture repeats the same sentence in two distinct
source blocks, so the test identifies the selected word by its exact anchor
and checks its hit region before and after changing viewport. The two
spread fixtures also verify independent leaf rasters, gutter hit rejection,
right-to-left links, Arabic word selection, note-marker offsets, and tap and
physical page-button events. Both returned portrait frames match their
original pixels. The Arabic test loads the original Noto Sans Arabic 2.013
font from the test fixtures. [Authored EPUB fixtures](../tests/fixtures) · [Font release](https://github.com/notofonts/arabic/releases/tag/NotoSansArabic-v2.013) · [Font license](../tests/fixtures/fonts/OFL-NotoSansArabic.txt)

## Fixed-page viewport preparation

`FixedReading::prepare_viewport` stages a fully rendered frame for paged and
continuous PDF, paged DjVu, still images, comics and fixed-layout EPUB. Paged
views keep the source point in the document's upright coordinates. The target
viewport recomputes the existing fit or tile policy and keeps the point as close
to the center as the crop and pan limits allow. Document rotation and crop
remain part of the source projection. Screen rotation belongs to the runtime.

DjVu uses the paged scan path for crop, embedded text selection, annotations
and tile zoom. Its document turn and straightening settings carry through the
resize. Scaled pages use bounded native DjVu patches sampled with the same
inverse transform as embedded text, so the ink and selection stay aligned.
A candidate for an earlier page is rejected after navigation. The continuous
scroll path is available for PDF.

Continuous PDF keeps the first visible cropped source point and any leading
inter-page gap. It applies per-page crops, hidden flows and the existing
end-of-document clamp, then checks that the source point is visible before
accepting the new frame. Comic spreads keep their selected leaf and physical
slots, including right-to-left spreads and empty slots for hidden pages.
Fixed-layout EPUB recalculates its publisher and reader spread rules for the
new dimensions and keeps the selected spine page.

Rendering, hit testing, annotation clipping and footer bounds use the prepared
viewport. The stage carries its frame and scratch rendering caches.
`commit_viewport` checks the reading session and the presentation generation
before replacing the active view. The landscape control remains disabled
while the Reader canvas, input and Launcher overlay handoff are integrated.

Run `cargo test --locked --lib fixed_session::` for the fixed-page transition
tests. Set `READER_FIXED_VIEWPORT_CAPTURE_DIR` to capture the authored PDF in
portrait, landscape and returned portrait in both paged and continuous layouts.
The tests verify projected links and annotations, manual crop and gap retention,
end-of-document clamping, rotated tile zoom, and the comic and EPUB spread rules.
The Mac renderer reproduced the original portrait frame after both transitions.
[Paged portrait](screenshots/fixed-viewport-portrait.png) ·
[Paged landscape](screenshots/fixed-viewport-landscape.png) ·
[Continuous portrait](screenshots/fixed-continuous-portrait.png) ·
[Continuous landscape](screenshots/fixed-continuous-landscape.png)

The same tests capture the original *Signal log* DjVu plate in both screen
sizes, a returned portrait frame and Reader's 480 × 640 source raster. The
native raster and embedded word coordinates can be checked independently
with `ddjvu -format=pgm -page=1 tests/fixtures/signal-log.djvu signal.pgm`
and `djvused -u -e print-txt tests/fixtures/signal-log.djvu`. Regenerate the
fixture with `python3 scripts/generate-djvu-viewport.py` using Python with
Pillow and DjVuLibre on the path. [Authored fixture](../tests/fixtures/signal-log.djvu) · [DjVuLibre tools](https://djvu.sourceforge.net/doc/)

[DjVu portrait](screenshots/djvu-viewport-portrait.png) ·
[DjVu landscape](screenshots/djvu-viewport-landscape.png)

## Reading controls in the prepared viewport

Reader uses the prepared document dimensions for the reading footer, Exit,
zoom, toolbars, Skim, and their hit areas. The Mac tests render the document
at 1264 × 1680 and 1680 × 1264. They exercise the lower-edge gesture on
the landscape canvas, verify that Exit remains reachable and leave the
original reflow source position intact on return to portrait. They also
check the PDF zoom and footer boundaries after resizing.

The width-scaled Canvas clipped landscape captions in the first
simulation. The negotiated Canvas uses the native panel’s short edge
as its scale reference. The new captures show the full chapter and page
caption above the progress track, with Exit at the right edge. The
Reader stages a document and draws the candidate before adoption.
Aborting the handoff leaves the accepted source and viewport intact.
The integrated Mac library passed 1,234 tests, with 66 opt-in tests
skipped. The Reader hook tests exercise reflow and PDF, a wrong-size
canvas, abort, retry, and switching documents while the display is
already in the requested direction. The supported controls include
Contents, Go to page, and saved marks, with both leaves laid out in a
facing-page EPUB.

A session with a negotiated Launcher display shows Landscape Left and
Landscape Right in the lower Screen panel and Quick Display. Selecting
a direction saves the edition preference after the runtime confirms the
prepared frame and the applied direction. Behind older Launchers the
existing portrait controls remain available. Documents and its Find screen use the active orientation when
Keep orientation is enabled. Vertical chapters and other scenes awaiting
their viewport updates follow Launcher and retain the edition preference
for later reading.

[Negotiated portrait options](screenshots/negotiated-orientation-portrait.png) ·
[Negotiated landscape options](screenshots/negotiated-orientation-landscape.png) ·
[Navigation in landscape](landscape-navigation.md)

The Mac simulation exercises the foreground contract with a test runtime.
The device gate includes native display readback, touch coordinates,
physical page buttons, sleep and USB resume, and return to Launcher.
[Shared viewport runtime](https://github.com/PrimitiveOS/kindle-primtitive-tools/tree/feature/app-viewport-runtime) ·
[Portrait](screenshots/reading-viewport-portrait.png) ·
[Landscape](screenshots/reading-viewport-landscape.png) ·
[Landscape toolbar](screenshots/reading-viewport-bars.png) ·
[Landscape Skim](screenshots/reading-viewport-skim.png) ·
[Landscape PDF](screenshots/reading-viewport-pdf.png)
