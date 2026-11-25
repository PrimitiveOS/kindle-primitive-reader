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
The mapper is not yet connected to the active reading layout or selection.

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
or active rendering. The canonical `Anchor` type has an optional raw caret slot;
ordinary serialization rejects populated slots, ordinary deserialization rejects
the field. The checked v2 library codec handles them separately. Legacy anchors
serialize as before. The existing version-1 loader in older builds rejects other
library/backup versions, which prevents those builds
from normally opening and rewriting a version-2 state. Startup also recovers
any pending backup journal before loading. A staged promotion verifies that the
v1 file still matches the exact bytes last loaded or successfully saved, retains a
digest-named v1 copy, and uses the existing recovery journal to publish a
validated v2 candidate. It rechecks the journal's captured old state and the
retained copy before commit. An interrupted or mismatched retained copy blocks
the transition; it is not silently replaced. Promotion also rejects duplicate
keys, legacy shapes, fields that would not round-trip, and states that v1
sanitization would change. The original file remains in place on rejection.
Filesystems that cannot sync
directories do not provide a guaranteed power-loss durability claim. Promotion is triggered only by a raw-bearing save; raw layouts remain disabled and this path has not been behaviorally tested.
A v2 decoder rejects duplicate JSON keys and raw fields
outside the known resume, bookmark, highlight, custom-contents and hidden-flow
anchors. It bounds file size and raw-caret count, checks each caret's structural
relationship to its canonical anchor, and keeps missing books without guessing
their source data. The canonical JSON portion must round-trip exactly through
Reader's state types, so unsupported fields or normalizations are rejected. An
encoder clones the state, detaches and validates raw carets, serializes
the canonical part, reinserts them at the approved paths and runs the decoder on
the result. A separate check can validate a raw caret against an open book's
source fingerprint and grapheme mapping. Reader can load a validated v2 library
without legacy sanitization and save it through the checked encoder and Kindle recovery journal. V2 libraries can preview and apply a conservative additive
merge from verified v1 or v2 backups; conflicting positions, marks, metadata,
contents or flow settings reject the entire merge instead of silently choosing
one. The multi-component restore verifies the captured disk baseline before
commit. Restoring a v2 backup into a v1 library uses the same strict conflict checks; it first retains the exact verified v1 state, journals any required pre-restore save against the loaded baseline, and rechecks both the journal capture and retained backup before commit. These checks coordinate Reader writes; they cannot provide an atomic compare-and-swap against an unrelated process writing the same file concurrently. An absent baseline or a v1 state that cannot be represented without loss is rejected. Raw resumes and history
locations are rejected before fixed or reflow page fallback. Operations using
an unjournaled generic save are rejected for v2, and a pending journal requires
Reader to reopen before saving its in-memory state. While raw rendering is
disabled, Reader refuses to open books containing any raw positions or marks
instead of exposing incomplete legacy editing paths. Raw rendering, app-level
raw restoration remains incomplete.
Archive moves of a v2 or raw-bearing library use the journaled save. If a save
fails and recovery is pending or cannot be inspected, Reader retains both the
source and staged archive copy; cleanup is deferred until recovery determines
which path the state references. This failure path has not been tested yet.
The decoder also validates book identities, retained collection
membership, position lists and mark limits without invoking v1 sanitization or
checking whether each book file is present. Raw-bearing books are explicitly
excluded from legacy metadata sidecar checkpoints, which cannot represent them;
the user receives a warning instead of a silently incomplete sidecar.
App-level resume, saved-mark validation and old ODT anchor migration reject raw
carets instead of silently using their collapsed canonical positions. Session
restoration can resolve one only when the newly shaped source and raw page
actually match. Mark comparisons and custom-contents/flow ordering account
for raw endpoints, but active reading cannot yet create or restore them.

A preformatted horizontal layout plan can now build a separate raw text view
for a verified simple block with an explicit `pre`, `pre-wrap` or
`break-spaces` mode. Every canonical byte must map in order to an identical raw
byte; additional whitespace is synthetic, and a collapsed space with no raw
space makes the plan fail closed. Images, ruby, inline markup, generated text,
directional controls, text transforms and other projections are currently
excluded. The plan is not activated: blank-line pagination, raw-aware selection
geometry and durable raw anchor storage must be integrated before it can safely
affect reading.

The detached plan can shape its raw text with Parley and create bounded,
grapheme-validated raw carets for contiguous line ranges, including lines
whose content is whitespace. It is tied to the source block's lifetime and
rechecks the raw snapshot and computed mode before shaping. A detached
selection-geometry consumer validates both raw anchors and the exact raw
quote before accepting bounded rectangles. Detached pagination can return a
verified raw anchor for the first line intersecting a paragraph-local page
range, including whitespace-only lines, and map an anchor back to a line using
its explicit boundary affinity. These are not installed into Reader's page model
or UI; durable state and the full integration remain open. A detached hit-test
can also resolve a shaped cluster to a verified raw grapheme caret, including
positions within restored whitespace. Empty areas without a cluster are rejected.
The chapter builder has a hard-disabled raw-layout branch that would transfer
verified source bindings and raw line boundaries into the page model. Page
lookup can map those lines to and from raw anchors with explicit affinity,
including blank lines; trailing blank-line extent is not trimmed. Raw layout
falls back when in-page notes are enabled. The branch remains disabled while resume, search and
state promotion are incomplete; active rendering still uses the legacy layout.
Literal, regular-expression and nearby search can consume a verified map of
the raw blocks actually accepted by a shaped chapter. The map checks book,
chapter, source revision, effective stylesheet configuration, line metadata
and raw fingerprints against the worker's source. Raw results carry exact
grapheme-checked endpoints; a raw nearby-search boundary without a verified
layout fails closed. The UI workers have a staged Send-safe layout recipe that
reconstructs a chapter in the worker, checks cancellation around source reads
and during layout, and supplies the map. Its full layout inputs are included
in search-result invalidation. This path is inactive while the raw layout gate
is off; search fails rather than guessing if a required map is missing.
Reader's selection value can carry a verified raw quote and raw endpoints,
including through the reading-session quote API. Within one section it can
also assemble cross-block quotes and geometry when endpoint layouts are
verified, including mixed raw and ordinary endpoints in different blocks,
using each intermediate block's installed projection. Cross-section raw
selections can assemble source-verified chapter fragments; saved geometry
revalidates the full quote before drawing its local fragment. This may require
reloading other sections and has not been performance-tested. Chapters with
joined notes and unsupported renderer projections still fail closed. Mixed endpoints in
one block are accepted only when both directional projections prove the same
raw position; ambiguous whitespace is rejected. The active renderer has not
yet installed raw layouts.
Selections whose ordinary endpoints surround a raw paragraph also route through
the raw projection, so intervening whitespace is not silently normalized.
Word extension uses source-verified ordering, and dictionary context follows
each displayed block's projection. Unsupported foreign-note and cross-section
raw ranges with unsupported note projections remain fail-closed.

This stage does **not** change rendering, copying, selection, source revisions,
existing anchors or saved state. It does not yet implement preformatted EPUB
rendering. Later stages need a bounded style-aware layout projection and a
backward-compatible way to represent positions inside raw whitespace. They
must handle whitespace-only lines, selection/copy, old highlights and resumes,
CSS changes and ambiguity without inventing a canonical location. Any
unsupported or unverifiable projection must continue to use the current
layout. This provenance has not yet been built or behaviorally tested.

[Pinned CRE whitespace formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2569-L2575)
