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

**Enforce steady line heights** provides a bounded subset of KOReader's
strut-confined setting. On ordinary horizontal paragraphs, simple directly
shifted text such as a subscript or superscript does not enlarge the existing
line box, while its real ink bounds remain available for painting and page
layout. Complex mixed-style atoms, deeper style subtrees, first-line styling,
initial letters, generated content, ruby and images retain ordinary sizing.
Tightly spaced text can overlap neighboring lines; the existing emergency
layout fallback remains in effect for unreadable page-height overlaps. This
is an opt-in presentation setting and does not alter source text or saved
anchors. Simulator, Kindle and KOReader comparisons have not yet been run.
[Pinned KOReader setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L466-L476)

**Document direction RTL** and **Document direction LTR** are mutually
exclusive overrides on the document body. They are intended for books with
missing or incorrect base direction, while supported block descendants with
explicit `dir` attributes and CSS can still establish their own direction. The horizontal
Parley reader uses the computed direction for paragraph shaping, supported
text-box geometry and native HTML table column ordering. Table direction is
projected onto the visual grid; automatic direction or ambiguous table
ownership retains the original grid. Nested native tables still use the
existing source-order fallback. Advanced inline bidi, vertical text and Blitz
rendering do not yet fully follow this CSS setting. Document text, revisions
and saved source anchors remain unchanged; the visual result has not yet
been tested. [Pinned KOReader controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L312-L340)

For ordinary horizontal paragraphs, Reader supplies virtual directional
isolates for inline HTML `dir` and `bdi`, and directional overrides for
`bdo`, following KOReader's control-character approach. The controls exist
only during bidi analysis: document text, selection offsets, copied text and
saved anchors never contain the synthetic characters. Nested hints and hard
line breaks use bounded, validated metadata; if ownership, nesting or limits
cannot be verified, Reader falls back to its previous shaping behavior.
Inline images, joined or boxed/generated text, CSS `unicode-bidi`, vertical
and Blitz layout are not covered. This implementation has not yet been built,
compared with KOReader or exercised in the simulator or on Kindle.

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

The horizontal reader also accepts publisher and user CSS `letter-spacing`
on ordinary styled runs. It supports `normal`, signed CSS lengths, inheritance
and KOReader's percentage-of-declaring-font extension. The reader's own letter
spacing control adds its tracking to the computed CSS value. To keep malformed
or extreme styles bounded, the publisher contribution is limited to
−1–128 pixels and combined negative tracking to −1 pixel; non-finite values
fall back to zero. `::first-letter` and `::first-line` may specify their own
spacing, including inherited first-line descendants; absolute units follow the
reading DPI. Relative pseudo-element lengths use Reader's existing pseudo font
scale, which may differ from KOReader's logical-font metrics. Text and source
anchors remain unchanged. Ruby annotations use the same computed spacing and
tracking as ordinary runs. Relative letter spacing declared directly on ruby
without an explicit ruby font size can differ from the fallback visual font
scale. Pseudo-element reset keywords, generated-content declarations,
vertical text and Blitz rendering do not yet have equivalent support. The
visual result and precise KOReader parity have not yet been tested.
[CRE inherited value](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L11524-L11526) · [CRE text layout](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L4568-L4574)

**Ignore publisher line-break restrictions** applies KOReader's non-important
`line-break: -cr-loose` rule. The horizontal reader implements a conservative
subset: non-breaking spaces and non-breaking hyphens between alphanumeric
characters may wrap after the character, at a grapheme boundary. Explicit
publisher line-break declarations can override the rule, and nowrap runs retain
their existing behavior. Other punctuation, complex contexts, generated text
and vertical or Blitz rendering are not covered. If a paragraph exceeds 4096
eligible boundaries, Reader keeps its ordinary line rules. The characters,
copying, selections and saved positions remain unchanged. The visual result
has not yet been built or tested. [KOReader setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L354-L363) · [CRE character treatment](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L799-L805)

**Sans-serif font for ruby** requests Noto Sans CJK SC, falling back to a
sans-serif family when unavailable, and applies regular weight. **Larger ruby
text size** sets ruby annotations to 50% of their parent size. Horizontal
Parley ruby now uses the annotation element's computed font, weight, slant
and explicitly declared size. Without an annotation size declaration, the
existing 48% fallback remains. Reader's font override and bold-text settings
still apply, and complex ruby or other renderers are not covered. The
annotation styling leaves source text and saved anchors unchanged. Simulator
and Kindle behavior have not yet been verified. [Pinned ruby tweaks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L384-L400)

An annotation wider than its base may extend into the neighboring annotation
space when it fits inside the reading column. Reader avoids overlapping two
painted ruby annotations on the same line; when their centered positions
collide, it looks for the nearest free position whose center remains over its
own base. An annotation with no such position, one wider than the column, or
one without sufficient vertical clearance is omitted. This bounded layout does
not widen or reflow the base to fit the
annotation. Complex ruby arrangements remain outside the supported subset.
These changes have not yet been built or behaviorally tested.
[CREngine ruby boxing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L8236-L8268)

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
