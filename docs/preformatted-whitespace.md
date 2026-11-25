# Preformatted whitespace provenance

Reader currently normalizes EPUB/XHTML text before the CSS cascade. Once
multiple spaces, tabs or line breaks have been collapsed, existing anchors and
highlights refer to those normalized bytes. Pinned CRE keeps text available to
its renderer and applies the computed `white-space` mode while formatting it.

The first implementation stage records an optional in-memory raw snapshot for
a leaf block containing exactly one direct text node. It independently repeats
Reader's existing normalization and retains the snapshot only when the result
matches the canonical block byte-for-byte. Non-whitespace spans map exactly to
canonical bytes; collapsed, leading and trailing whitespace are recorded as
explicit ambiguous gaps rather than assigned guessed anchor offsets. Each
snapshot is limited to 64 KiB and 4096 span/gap records. A chapter has separate
768 KiB preferred `<pre>` and 256 KiB general metadata budgets, including
allocated record capacity; a block outside those budgets uses the existing
path. Markup, multiple text nodes, hidden/excluded content and other complex or
nonmatching blocks are not captured.

A bound snapshot verifies the canonical block with SHA-256 before mapping.
Canonical carets map to gap edges using explicit before/after affinity; raw
carets on exact text or whitespace edges map back exactly. An interior raw
whitespace caret is returned as an explicit gap with its raw offset rather
than being silently snapped to a normalized position. Invalid UTF-8
boundaries, mismatched canonical text or conflicting mappings fail closed.
The mapper is not yet connected to layout, selection or saved anchors.

This stage does **not** change rendering, copying, selection, source revisions,
existing anchors or saved state. It does not yet implement preformatted EPUB
rendering. Later stages need a bounded style-aware layout projection and a
backward-compatible way to represent positions inside raw whitespace. They
must handle whitespace-only lines, selection/copy, old highlights and resumes,
CSS changes and ambiguity without inventing a canonical location. Any
unsupported or unverifiable projection must continue to use the current
layout. This provenance has not yet been built or behaviorally tested.

[Pinned CRE whitespace formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2569-L2575)
