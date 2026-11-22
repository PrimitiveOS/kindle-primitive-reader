# PrimitiveOS text layout changes

This directory contains Parley 0.12.0, from the published Rust crate and
[upstream revision 6e1143dda817360dd3a2a64f9133c6e5dfabd881](https://github.com/linebender/parley/tree/6e1143dda817360dd3a2a64f9133c6e5dfabd881/parley).
The upstream MIT and Apache-2.0 license files apply. The patch uses the existing
Skrifa 0.44.0 dependency version for the selected font instance.

The ranged builder's optional callback receives the original UTF-8 offset of
an isolated U+00AD. When eligible, the line breaker admits that boundary only
if the selected face's hyphen advance fits. The committed line paints that
glyph at the original cluster and includes its advance in caret geometry.
The callback can suppress the discretionary opportunity or keep Parley's
ordinary break decision without drawing a hyphen.
The same builder accepts sorted byte offsets for dictionary breaks. At a complete
one-character LTR shaping atom, the chosen break appends a measured hyphen
after the original glyphs. Line, run, cluster and positioned-glyph advances
include the same decoration. Other builders retain the upstream behavior.

Manual hyphenation covers opportunities represented by standalone zero-advance
clusters, with U+2010 or U+002D in the original face. Each layout considers at
most 4096 authored markers. It retains the full shaped paragraph for bidi
and joining context. Language-specific spelling changes, fallback to a
different face, splitting ligatures and RTL dictionary decoration remain future
work. Reader supplies bounded English dictionary offsets. [Language coverage](../../docs/english-hyphenation.md) The callback is used for line layout at a supplied width. Intrinsic
minimum-width measurement still follows upstream Parley.

Run the regression suite with
`cargo test --manifest-path vendor/parley/tests/Cargo.toml --test soft_hyphen`.
The Latin tests use the Reader's Silkscreen fixture. The additional macOS bidi
tests use the system's Arial Unicode MS.

Reader also supports a baseline shift expressed as fractions of the parent font
height and descent. Paragraph layout uses this for superscripts and subscripts
and their nested inline content. The existing built-in shift values retain
their original semantics. [Layout comparison and tests](../../docs/inline-text-baselines.md)

The opt-in justification mode can distribute part of the surplus line width
within words. It uses the original shaping atoms, leaves cursive and ideographic
text at its normal spacing, and gives layout, selection and painting the same
adjusted advances. The standard alignment API keeps its upstream behavior.
[Reader word expansion](../../docs/word-expansion.md)

The ranged builder also accepts an optional per-cluster spacing policy. It
passes each shaped atom's original text, natural advance and font size to
the caller. Extra advances are measured before wrapping and shared by line
measurement, painting and selection. Reader applies this to horizontal CJK
text. [CJK width and geometry checks](../../docs/cjk-width.md)

The ranged builder accepts an optional signed font-weight advance callback.
Reader supplies the remaining weight difference after face selection, including
a publisher's declared face class. Fractional advances use the same before/after
cluster-spacing representation as CJK widths. Wrapping, intrinsic table widths,
caret positions, painting and word hits consume those shared gaps. Zero-advance
marks retain their original geometry. With no callback, upstream behavior is
unchanged. [Numeric contrast and weight](../../docs/font-contrast-and-weight.md)
