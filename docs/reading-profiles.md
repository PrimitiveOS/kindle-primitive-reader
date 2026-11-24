# Reading profiles

Open a reflowable book, PDF, DjVu document or comic and choose **Appearance › Reading profiles** from the top bar. The lower **Text** and **View › Layout** controls also lead to the same page. Set up the book as you like, then use **Save current settings** and give the profile a name.

Tap a profile to **Apply to this book**, **Replace with current settings**, **Included settings**, **Rename**, **Duplicate**, or **Delete**. Replacing and deleting ask for confirmation. Reader lists all supported profiles together and enables each Apply or Replace action only in the matching format. Profiles apply to the open book on demand. [Document defaults for new books](document-defaults.md) and [Status bar presets](status-bar-presets.md) are separate.

Open **Included settings** and use the switches to choose which settings the profile applies. **Clear all** helps you start with a small selection. **Save** requires at least one setting. **Cancel** discards the draft. Applying a profile changes only its selected settings. **Replace with current settings** updates the selected settings in the saved profile. Excluded values stay stored in case you include them again. Earlier snapshots start with all settings included.

| Format | Settings saved |
| --- | --- |
| Reflowable books | Font and size, line and word spacing, margins, alignment, typography, publisher styles and fonts, image scaling and night images, single or facing pages, and scroll or page turns. Older text profiles keep their existing names and values. |
| PDF documents | Page fit and zoom, row or column tile count, PDF page rotation, scroll and spacing, scan contrast, and pan overlap and order. Tile magnification is calculated using the target page geometry. Each PDF retains its own saved crop area, OCR policy and annotations. Crop margin settings can update an automatically calculated crop. A mode that would disable an active continuous crop is refused. |
| DjVu documents | Fit and tiled zoom, page rotation, pan overlap and order, adaptive contrast, white level, deskew, dithering and reflow line spacing. |
| Comics | Fit and zoom, rotation, facing pages, reading direction, pan settings, scan contrast and dithering. A profile applies to CBZ, CBT or CBR through the same provider. |

Applying a profile preserves the current book's page or source anchor and annotations. A facing-page change that would switch the current page is refused. PDF pan offsets stay within the limits of the new view. Reader prepares the new layout, checks the document edition and saves the settings before changing the visible page. Shared profiles live in `reader/data/reading-profiles.json` with up to 16 case-insensitively unique names. If the file is unreadable or has changed since opening Reader, the existing file is preserved. Reading-data backups use the same validation and accept the original reflow-only file. [Backup format](reading-data-backups.md)

## Comparison with KOReader

The pinned stock **Profiles** entry appears in Tools on frame 006 of the SDL reference. **New with current book settings** captures different action lists for rolling and fixed documents. The stock editor can change or remove individual saved actions, and Execute dispatches supported actions for the open format in order. Reader covers the current-book capture, explicit apply, field selection and profile management for reflowable books, PDFs, DjVu and comics. Reader can edit font size, content zoom, line spacing, word spacing and expansion, page margins, font weight and contrast, zoom overlap, and supported scan controls directly. Other values can be copied from the current book with Replace. Stock also offers editable values for additional actions, profiles for other fixed formats, empty profiles, automatic execution and a general action list. [Stock factory and commands](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/profiles.koplugin/main.lua#L98) · [Stock format settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/profiles.koplugin/main.lua#L346) · [Stock action editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L1054) · [Stock apply and format gating](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L1372)

The executable fixture runs stock snapshot creation, the font size, content zoom, line spacing, word spacing and expansion, page margins, font rendering, and overlap editors with their shipped defaults, action removal, duplicate, rename and confirmed deletion, and batched application with format gating. Run with LuaJIT and the pinned source directory as the sole argument. Desktop Reader tests use the production lists, keyboard submit, confirmation and rendering paths with an authored EPUB and two PDFs of different page sizes. They cover stored-profile compatibility, source and position guards, protected crop geometry, retry after failed saves, typed backup validation, draft cancellation and preservation of excluded fields. The DjVu and CBZ tests use the production providers. [Stock fixture](../scripts/koreader-reading-profiles-check.lua) · [Desktop workflows](../src/app/profile_panel/tests.rs)

[Profile list](screenshots/reading-profiles.png) · [PDF profile options in dark mode](screenshots/reading-profile-pdf-dark.png) · [Delete confirmation](screenshots/reading-profile-delete.png)

[Text setting selection](screenshots/reading-profile-settings.png) · [DjVu setting selection in dark mode](screenshots/reading-profile-settings-djvu-dark.png)

## Editing saved values

Open a profile and tap **Edit values**. Reflowable profiles have **Font
size**, **Content zoom**, **Line spacing**, **Word spacing**, **Word
expansion**, **Page margins**, and **Font rendering**. **Book rendering**
groups publisher styles, publisher fonts, image scaling, and night images.
Fixed-page profiles have **Horizontal overlap**, **Vertical overlap**, and
scan controls for supported formats. Tap a numeric value to type it or use
its step buttons.
**Set** saves the value in the profile.
**Back** discards the draft.
The open book keeps its settings until **Apply to this book** is used.
Edit values keeps the existing Included settings selection, including
the single **Zoom pan overlap** switch that governs both axes.

The pinned KOReader profile editor offers font sizes 12–44, with a 22
default and steps of 0.5 on tap and 5 on hold. Its Content zoom choices
are 0, 48, 96, 167, 212, and 300 dpi. Reader keeps its regular font size
range of 12–255 and accepts any whole Content zoom value from 0 to 300
dpi, with a 96 dpi default. A pixel-only font preset shows its older size
in the list and starts the editor at the nearest logical half-unit.
Set stores the logical value while preserving the old pixel fallback.
[Landscape font editor](screenshots/landscape-profile-font-size.png)

**Word spacing** edits the scaling and minimum remaining width together.
**Set** saves the pair in one transaction. A legacy additive-pixel value
stays in the profile and is used until a percentage pair is saved.
**Word expansion** is a separate percentage and Included settings switch.
Profile editing keeps the source file, current position and visible book
frame. The selected settings take effect after Apply.

The KOReader profile dispatcher offers spacing pairs 75/50, 95/75 and
100/90, and expansion presets 0%, 5% and 15%. The pinned Mac runtime's
exact reading control also accepted a 123/67 spacing draft and expansion
of 7%. Reader's saved editors support the same exact reading bounds.
Scaling accepts 10–500%, reduction accepts 25–100%, and expansion accepts
0–20%. Hold steps are 10 percentage points for either spacing value and
4 for expansion.
[Stock reading options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L457)
· [Landscape saved word spacing](screenshots/landscape-profile-word-spacing.png)

**Page margins** edits the four stored edges in pixels. Older profiles
use their existing all-edge or side fallback wherever an edge has no
saved override. Linking top and bottom initializes both to their rounded
average. Editing either linked edge then updates both. The profile's
**Margins** switch still controls whether the values apply to a book.
Values use the profile storage range of 0 to 1024 px. Apply checks margins
against the target book's current viewport, as it does for other saved
margin settings.
[Landscape linked-margin editor](screenshots/landscape-profile-bottom-margin.png)

KOReader's profile dispatcher offers nine left/right pairs from 5 to 140,
with separate top and bottom spinners from 0 to 140, a default of 15 and
a held step of 5. Its link flag is a separate action. KOReader scales
these logical values to the screen when using them. Reader stores exact
pixels for each edge.
[Stock margin options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L134)
· [Stock margin application](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypeset.lua#L476)

**Font rendering** edits the saved **Font weight** and **Font contrast**
independently. Weight is an adjustment from −3 to +5.5, with a 0.25 step
and a held step of 1. Contrast uses Reader's 57 gamma levels from 0.30 to
15.00. The buttons step through the table, and an exact table value
can also be typed. A saved legacy Bold or High contrast flag remains in
use until Set commits a new value for that field. Included settings
controls which of the two values Apply uses.

The stock profile callbacks verified seven weight presets and eight
contrast presets. The pinned Mac runtime's exact reading controls
verified weight 0.75 and gamma 1.25, including one-level and five-level
contrast stepping and the stored index of 21 for gamma 1.25.
[Stock font rendering options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L571)
· [Landscape saved contrast](screenshots/landscape-profile-font-contrast.png)

**Font rendering** also offers Off, Native and Auto for hinting and Off,
Fast, Good and Best for kerning. **Book rendering** lets you choose whether
Publisher styles and Publisher fonts are on, whether image scaling is Fast
or Smooth, and whether images stay Original or are Inverted in dark mode.
Tap a choice to save it in the profile. Use **Apply to this book** to
activate the included settings. A copied profile can set a night-image
preference from either reading theme.

The pinned profile dispatcher selects and saves all six options and
dispatches them when the profile is executed on a reflowable book. The
Mac workflow checks legacy on/off kerning, inherited defaults, cancelled
selection, a conflicting file write, the included-setting mask and the
saved reading position in landscape.
[Landscape font choices](screenshots/landscape-profile-rendering-choices.png) ·
[Landscape book choices](screenshots/landscape-profile-book-rendering.png) ·
[Pinned text options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L629-L745) ·
[Executable comparison](../scripts/koreader-reading-profiles-check.lua)

For PDF profiles, **Crop margin** accepts 0.00–1.50 inches, with a 0.10
default. Tap to step by 0.01 or hold to step by 0.10. Apply updates the
padding around detected bounds in Auto or Semi-auto crop and keeps the
book's own crop mode and seeds. Reader reprojects the current source
position into the new bounds. Page edges can limit the resulting position.

For PDF and comic profiles, **Scan contrast** accepts gamma 0.8–50.0,
with a 1.0 default. Tap to step by 0.1 or hold to step by 1.0. A profile
with saved **Adaptive** contrast keeps that setting until you commit a
numeric value with Set. DjVu continues to save its Normal or Higher
contrast through Save current settings or Replace. Crop margin and Scan
contrast each have their own Included settings switch.

The stock profile dispatcher offered eight presets for both actions and
applied the stored values only on execution. The pinned Mac runtime's
PDF spinners used the same exact bounds and steps and applied a margin
of 0.19 and gamma of 1.9. Reader offers direct entry at that precision in
saved profiles as well. [Stock crop margin](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L123)
· [Stock scan contrast](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L489)
· [Landscape saved crop margin](screenshots/landscape-profile-scan-margin.png)

The pinned callbacks also tested line spacing from 50% to 200% with a
100% default and independent overlap values from 0% to 84% with a 36% default.
Both apps step by one percentage point on a tap and five on a hold.
Reader accepts overlap up to 90% to retain previously valid profiles,
and uses 36% as the default in these profile editors. Reader keeps
excluded values for later use. KOReader enables an action when its saved
value is edited and can remove each overlap action separately.
[Stock value editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L902)
· [Stock overlap defaults](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L161)

The host tests cover legacy percentages, typed values, invalid input,
cancellation, conflicts with changes on disk, and an unaccepted landscape
frame. Saved values take effect through the existing profile application
path, which preserves the EPUB source anchor and PDF page.
[Landscape overlap editor](screenshots/landscape-profile-overlap.png)

## Using profiles in landscape

The list, profile details, Included settings, name editor, and confirmations
follow the active reading orientation. Profile lists retain the first
visible entry through rotation. Included settings keeps Save and Cancel
beside the heading. Name editing uses the shared keyboard at the active
width. Replace and Delete stay centered at the same physical scale.

The pinned KOReader Mac runtime at 1680 × 1264 kept a book open through
the native name keyboard's Cancel and Save actions and confirmed deletion.
The source XPointer remained identical. Reader retains its existing
Save current settings, Included settings and Back behavior in both turns.
[Stock name editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/profiles.koplugin/main.lua#L313)

Run `cargo test --locked --lib app::profile_panel` to exercise the host
workflows. The tests cover saving selected EPUB settings, rejecting an
empty selection, discarding an uncommitted draft, renaming, duplicating,
replacing and deleting PDF profiles, and returning to the book location.
A viewport round trip restores the same visible profile list in portrait.

Applying a text profile prepares the book at its accepted viewport size.
The chapter caption and Exit remain visible after applying 122% line
spacing in a landscape EPUB. The host checks also keep the same book
anchor through a text-default reset, and retain the visible footer when
toggling document overlap in an EPUB or PDF.
[Reading page after Apply](screenshots/landscape-profile-applied.png)

Set `READER_LANDSCAPE_TOOLS_CAPTURES` to save the desktop render.
[Included settings in landscape](screenshots/landscape-reading-profile-fields.png)


### Oasis profile Apply retest

The first ARM build saved the new EPUB line spacing independently of the
active book, then revealed a portrait-sized document frame after Apply in
Landscape Left. The landscape footer caption and Exit disappeared. The
corrected build `9369dca` uses the accepted document dimensions when it rebuilds
the page. It saved a 124% value and preserved the profile's included field
and the open book data until Apply. Apply changed the reading pixels and
kept the exact source anchor. Framebuffer inspection confirmed ink in the
landscape-only region and both footer controls. A tap on the far-right Exit
returned to Launcher.

The same integrated source passed 1,293 Mac library tests, one Kindle entry
test, 14 EPUB package tests and four layout checks. The reflow-default and
EPUB/PDF footer-overlap regressions check the whole frame and the Exit target.
[Oasis frame after Apply](screenshots/oasis-landscape-profile-applied.png)
