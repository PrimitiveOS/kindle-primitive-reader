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
