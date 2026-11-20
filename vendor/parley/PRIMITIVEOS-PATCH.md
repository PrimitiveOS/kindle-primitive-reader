# PrimitiveOS soft-hyphen patch

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
Other Parley builders retain the upstream behavior.

Manual hyphenation covers opportunities represented by standalone zero-advance
clusters, with U+2010 or U+002D in the original face. Each layout considers at
most 4096 authored markers. It retains the full shaped paragraph for bidi
and joining context. Language-specific spelling changes, fallback to a
different face, splitting ligatures and automatic dictionaries remain future
work. The callback is used for line layout at a supplied width. Intrinsic
minimum-width measurement still follows upstream Parley.

Run the regression suite with
`cargo test --manifest-path vendor/parley/tests/Cargo.toml --test soft_hyphen`.
The Latin tests use the Reader's Silkscreen fixture. The additional macOS bidi
tests use the system's Arial Unicode MS.
