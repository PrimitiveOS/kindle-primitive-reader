# Publisher media-type rules

The native EPUB CSS candidate evaluates publisher `@media` blocks containing
simple media types. `screen` and `all` match, `print` does not, and `not` or
`only` may precede a supported type. Comma-separated supported queries are
alternatives. This follows the pinned CRE engine's screen-medium choice:
[media matching](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L2447-L2494).

Matching nested rules and font faces retain source order, subject to the
existing stylesheet, face and rule budgets. Nested `@import` is ignored. The
parser admits at most four nested media blocks and 256 parsed rules per
stylesheet. Unsupported or malformed conditions fail closed: viewport,
orientation, color, resolution and compound feature conditions do not match.
Unsupported alternatives are false without suppressing another supported,
matching alternative in the same comma-separated list. Strict user CSS remains unchanged.
Source text, chapter revisions and saved positions are unchanged.

Publisher rules and imports also support a single `width`, `min-width`,
`max-width`, `height`, `min-height`, `max-height`, or `orientation` feature,
alone or after `screen`/`all`/`print and`. Size values are bounded nonnegative
integer CSS pixels (or unitless zero); orientation accepts `portrait` and
`landscape`. The effective leaf's text viewport and Render DPI are captured
for layout and reused by search, generated-quote flow, and note/Contents
helpers. Unsupported features, multiple conditions and feature negation remain
unmatched. Layout changes may repaginate without changing source positions.
Publication-wide embedded-font diagnostics have no viewport and can omit
faces declared only within these queries; the active layout still registers
matching faces. This increment has received source review only and needs
behavioral and device validation.

HTML head `link` and `style` elements also exclude a stylesheet when their
`media` attribute contains only known nonmatching simple media types, such as
`print` or `not screen`. A missing or empty attribute, a matching alternative,
or an unknown, malformed or over-1,024-byte condition keeps the earlier
stylesheet behavior. This conservative filter avoids suppressing a publisher
sheet on an unsupported feature query; it is not full media-query evaluation.
An affected book can repaginate, while source text and saved anchors stay
unchanged. Chapter-discovered fonts use the filtered stylesheet list; the
publication-wide font inventory can still report faces from manifest sheets.

Top-level local `@import` rules can also use these simple media types, for
example `@import url("reading.css") screen;`. Matching imports retain their
position in the cascade and use the existing relative-path, cycle, import-count,
depth and byte limits; nonmatching or unsupported media conditions do not
load a stylesheet. Unqualified imports retain their previous behavior. Applying
a newly matched publisher stylesheet can change formatting and pagination;
the original source and text anchors do not change. Media features, `layer()`
and `supports()` import qualifiers remain unsupported.

This is an implementation candidate. Cascade behavior, nested blocks, font
registration, resource limits, KOReader comparison, real EPUBs, simulator and
Kindle behavior have not yet been behaviorally validated.
