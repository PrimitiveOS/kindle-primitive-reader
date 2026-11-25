# Reading style presets

Open **Settings › Style tweaks** to apply a change to the open book or
set a default for other books. A book can override the default, and turning
off style tweaks preserves the saved choices.

**Tailor widths and text-indent for CJK** centers a whole-cell line width
and rounds the first-line indent to the same cell size. The cell size
tracks **CJK character width**. A 557px text area at 40px type uses 520px
at 100% and 550px at 125%. A 1.2em indent becomes 80px or 100px,
respectively. Reader preserves the source text and saved position as it
wraps. Float contours, inline illustrations, and columns narrower than
one cell retain their ordinary layout. Vertical text uses its own
layout path.
[Reader setting](screenshots/reader-style-grid-controls.png) ·
[Pinned style definition](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L527-L533) ·
[Native width calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8911-L8925)

**No indentation on following paragraphs** applies only to a paragraph
immediately following another paragraph in the same container. The first
paragraph and a paragraph after a heading keep their existing indentation.
[Pinned indentation controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L537-L565)

## Text size and line height

**Normal line height** offers KOReader's 0.90–1.50 choices in 0.05 steps.
The choices are mutually exclusive and can be saved for one book or as a
default. They override only computed `line-height: normal`; an explicit
publisher numeric line height remains in force. The reading line-spacing
control still scales the chosen value. Reader may increase spacing in its
existing emergency fallback when font metrics would otherwise make a page
unreadable. Horizontal Parley layout supports this setting; vertical and
Blitz rendering remain outside this subset. Saved source anchors do not
change. This addition has not yet been built or behaviorally tested.
[Pinned KOReader choices](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L486-L505)

Four additional per-book and default choices use the supported native CSS
cascade:

- **Ignore publisher line heights** inherits line height across elements so
  the reading line-spacing setting has more influence.
- **Larger spacing between ruby lines** applies a line height of 2 to
  paragraphs and list items, including paragraphs without ruby so their
  spacing remains more consistent.
- **Reset main text font size** applies the root reading size to the body,
  paragraphs, list items, divisions and block quotations. It is mutually
  exclusive with **Ignore publisher font sizes**.
- **Smaller sub- and superscript** applies 50% font size and the matching
  subscript or superscript alignment.

The choices change presentation without rewriting the source document or its
saved anchors. They use Reader's existing CSS and layout subset; exact
KOReader typography, simulator behavior and Kindle appearance have not yet
been verified. [Pinned KOReader definitions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L402-L483)

**Disable common ligatures** uses KOReader's non-important user rule, so a
supported explicit publisher declaration can override it. Reader accepts
`normal`, `common-ligatures` and `no-common-ligatures` on `font-variant`,
`font-variant-ligatures` and its WebKit alias. In horizontal Parley text,
the setting changes the OpenType `liga` and `clig` features without disabling
required ligatures. Existing Fast and Good kerning policies remain in force.
Other font-variant combinations, pseudo-element-specific overrides, and
vertical or Blitz rendering are not covered by this implementation. The
source text and saved anchors are unchanged. This behavior has not yet been
built or tested. [Pinned KOReader tweak](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L364-L371)

**Sans-serif font for ruby** requests Noto Sans CJK SC, falling back to a
sans-serif family when unavailable, and applies regular weight. **Larger ruby
text size** sets ruby annotations to 50% of their parent size. Horizontal
Parley ruby now uses the annotation element's computed font, weight, slant
and explicitly declared size. Without an annotation size declaration, the
existing 48% fallback remains. Reader's font override and bold-text settings
still apply, and complex ruby or other renderers are not covered. The
annotation styling leaves source text and saved anchors unchanged. Simulator
and Kindle behavior have not yet been verified. [Pinned ruby tweaks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L384-L400)

**Allow hyphenation on all text** overrides publisher hyphenation restrictions
and excludes heading elements, using KOReader's selectors. Automatic
hyphenation still depends on the reading setting and an available language
dictionary or algorithm. **Left align all elements** and **Justify all
elements** apply their alignment across the document. These two choices are
exclusive with the existing paragraph-alignment choices; an explicit reading
alignment setting can still override publisher-style alignment. The original
document and saved anchors do not change. These additions have not yet been
built or behaviorally tested. [Pinned alignment choices](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L263-L295) · [Pinned hyphenation choice](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L347-L353)

## Heading page breaks

Six independent **New page on H1** through **New page on H6** choices use
the corresponding KOReader rules. A chosen heading starts a page unless it
immediately follows a higher-level heading, in which case the rule avoids a
break between the two. The previous combined H1/H2 choice remains available
for existing settings. These are layout rules: source text, revisions and
saved anchors stay unchanged. Horizontal Parley pagination supports these
rules; other rendering modes and the interaction of multiple settings need
simulator and Kindle verification. [Pinned KOReader rules](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L130-L175)

## Widows and orphans

**Avoid widows and orphans** requests two lines at both ends of a paragraph.
**Avoid widows but allow orphans** requests two lines at the top of the next
page but allows one line at the bottom of the previous page. These choices
are mutually exclusive with the earlier **Protect widows and orphans**
setting. **Ignore publisher orphan and widow rules** is independent and can
be combined with either choice. Reader applies the selected defaults to each
chapter body when both options are active; KOReader uses a `DocFragment`
element for that role in its EPUB tree. The earlier Protect setting retains
its stronger all-element behavior when combined with the publisher override.

The choices affect pagination without changing document text or saved source
anchors. Horizontal Parley pagination supports the relevant line-keeping
rules; other rendering modes and actual Kindle appearance remain unverified.
[Pinned KOReader controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L199-L241)

## Paragraph spacing and browser defaults

**Spacing between paragraphs** contains three mutually exclusive choices.
Full and half spacing add 1em or 0.5em before a paragraph that directly
follows another paragraph. No spacing clears the two vertical margins
of paragraphs. Each choice can apply to the open book or become a default.
**Generic web browser paragraph style** starts with start-aligned paragraphs,
zero first-line indent, and 1em top and bottom margins. Explicit publisher
styles can override those defaults. The spacing choices can override them.
[Reader menu](screenshots/reader-paragraph-spacing-controls.png) ·
[Pinned paragraph styles](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L510-L596)

The native replay invokes each stock style callback and measures consecutive
line origins, the paragraph after a heading, and an authored 2em margin and
indent. The stock full and half choices add 40px and 20px at a 40px type size.
Browser style retains the authored 80px gap and indent. Reader tests measure
the same paragraph gaps directly between line boxes, then reopen a saved
per-book override at the original location. The geometry covers ordinary
horizontal Parley blocks. The vertical layout and the remaining publisher
box modes have separate comparisons.
[Stock replay](../scripts/koreader-paragraph-spacing-check.lua) ·
[Shared specimen](../tests/fixtures/reading-paragraph-spacing.xhtml)

Run the stock replay inside the pinned native runtime with absolute paths
to the XHTML specimen, an empty CRE cache directory, the Noto Sans Regular
fixture font, and an output directory. Run the two Reader checks with
`cargo test --locked --features kindle --lib reading_style_paragraph`.

| Mode | Reader | KOReader |
| --- | --- | --- |
| No spacing | [Page](screenshots/reader-paragraph-spacing-none.png) | [Page](screenshots/stock-paragraph-spacing-none.png) |
| Half spacing | [Page](screenshots/reader-paragraph-spacing-half.png) | [Page](screenshots/stock-paragraph-spacing-half.png) |
| Full spacing | [Page](screenshots/reader-paragraph-spacing-full.png) | [Page](screenshots/stock-paragraph-spacing-full.png) |
| Browser paragraph style | [Page](screenshots/reader-paragraph-spacing-web.png) | [Page](screenshots/stock-paragraph-spacing-web.png) |

## Monochrome

**Pure black and white** clears publisher background fills and uses black
for the supported borders, including fitted text boxes and table cells.
Reader's regular text ink, inline images, and annotations keep their
own paint behavior.
[Pinned black-and-white style](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L836-L847)

## Simulator comparison

The pinned KOReader replay uses the real style callbacks and CRE renderer.
It measures the cell width, starting positions, source location, and line
endpoints at 100% and 125%. The color specimen checks the border and
background pixels before and after the preset. Reader tests use the same
specimens and exercise the saved menu choice, the reading anchor,
selection, and search. The Reader color test uses its Web EPUB render
mode so the specimen's fitted border is painted. Each engine retains its
own pagination and glyph rasterization.
[Stock replay](../scripts/koreader-reading-style-grid-check.lua) ·
[Reader layout tests](../src/parley_layout/style_presets_tests.rs)

| Mode | Reader | KOReader |
| --- | --- | --- |
| CJK grid at 100% | [Page](screenshots/reader-style-grid-100.png) | [Page](screenshots/stock-reading-cjk-grid-100-on.png) |
| CJK grid at 125% | [Page](screenshots/reader-style-grid-125.png) | [Page](screenshots/stock-reading-cjk-grid-125-on.png) |
| Publisher fills and borders | [Page](screenshots/reader-style-monochrome-off.png) | [Page](screenshots/stock-reading-monochrome-off.png) |
| Pure black and white | [Page](screenshots/reader-style-monochrome-on.png) | [Page](screenshots/stock-reading-monochrome-on.png) |
