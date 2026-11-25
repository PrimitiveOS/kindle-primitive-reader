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

An internal, serializable raw-caret value now records a raw offset, its
canonical edge, explicit affinity and the raw text's SHA-256. The bound mapper
can create and validate this value without snapping gap interiors, and reject
carets inside grapheme clusters. A separate internal raw-anchor envelope pairs
it with an unchanged canonical `Anchor`; its validator checks the supplied
book, chapter, revision and block identity, snapshot block key, both content
fingerprints and the canonical/raw relationship. The envelope is not yet
attached to persisted state or used by rendering. Internal Probe methods can
now create and validate transient raw anchors against the currently open
publication, or project an existing canonical anchor to a raw edge without
rewriting the saved anchor. A transient selection consumer can validate two
raw anchors in the same captured block and return their exact raw substring;
cross-block and invalid selections fail closed. It is not connected to the UI
or persistence. Before raw carets are
persisted, the library and backup format must support version 2, preserve an
atomic version-1 backup and reject unsupported raw data. The existing version-1
loader rejects other library/backup versions, which prevents older builds
from normally opening and rewriting a version-2 state. Startup also recovers
any pending backup journal before this check, so the eventual migration must
coordinate its transaction with that recovery and keep the independent backup.

A preformatted horizontal layout plan can now build a separate raw text view
for a verified simple block with an explicit `pre`, `pre-wrap` or
`break-spaces` mode. Every canonical byte must map in order to an identical raw
byte; additional whitespace is synthetic, and a collapsed space with no raw
space makes the plan fail closed. Images, ruby, inline markup, generated text,
directional controls and other projections are currently excluded. The plan is
not activated: blank-line pagination, raw-aware selection geometry and durable
raw anchor storage must be integrated before it can safely affect reading.

This stage does **not** change rendering, copying, selection, source revisions,
existing anchors or saved state. It does not yet implement preformatted EPUB
rendering. Later stages need a bounded style-aware layout projection and a
backward-compatible way to represent positions inside raw whitespace. They
must handle whitespace-only lines, selection/copy, old highlights and resumes,
CSS changes and ambiguity without inventing a canonical location. Any
unsupported or unverifiable projection must continue to use the current
layout. This provenance has not yet been built or behaviorally tested.

[Pinned CRE whitespace formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2569-L2575)
