# Preformatted whitespace

The local Reader build enables a bounded horizontal layout for simple source
blocks with `white-space: pre`, `pre-wrap` or `break-spaces`. A bounded
`pre-line` content also uses a bounded source-mapped collapse projection for
horizontal whitespace and line breaks, including bounded explicit `<br>`
elements. Collapsed whitespace crossing inline boundaries keeps the first
space's computed style, while later collapsible spaces have no width.
Unsupported mappings, form-feed controls and unverifiable boundaries retain the
canonical fallback. A carriage return retained in the parsed document is
treated as a space; pre-line collapses it, while pre, pre-wrap and break-spaces
keep its one-byte position and surrounding preserved whitespace. When the projected text
already matches canonical text, Reader keeps the canonical layout and anchors. It preserves
verified spaces, tabs and line breaks while keeping existing canonical source
identities. This implementation has not yet been behaviorally tested in the
simulator or on Kindle and must not be treated as validated or released.
The adjacent-owner and retained-carriage-return behavior follows the
[CSS Text white-space processing rules](https://www.w3.org/TR/2024/CRD-css-text-3-20240930/#white-space-processing).

A snapshot is captured for direct text and a bounded set of inline elements
when the concatenated text normalizes to the canonical block exactly. Inline
links retain their existing canonical target ranges; raw glyph hits map back to
those ranges, and whitespace without an exact mapping is not treated as a link.
Style runs retain each source text node’s owner, including whitespace at
inline boundaries; each run keeps its computed style and language. Hidden,
unpainted, direction-changing or reclassified descendants, including empty
elements and unsupported breaks, use the canonical fallback. Explicit
`<br>` elements map to verified canonical newlines and a terminal explicit break
is retained in pre-line projections. Comments and processing instructions are ignored. Each block is limited to
64 KiB and 4,096 mapping records; chapter metadata budgets are 768 KiB for
preferred `<pre>` content and 256 KiB for other content. The ordinary layout
is used for content outside these bounds, unsupported projections, images,
ruby, generated content, unsupported inline styles, transforms, bidi controls, joined
notes and chapters with detected in-page note targets. The in-page-note setting alone
does not disable raw layout in chapters without note targets. A layout whose source or ordered font stack cannot
be stamped also uses the canonical path. A folder-backed EPUB can use an
immutable source snapshot prepared in the background and checked against its
library identity and current reading position. The snapshot includes all
members, is limited to 512 entries and 16 MiB, and rejects hidden members,
symlinks, unsupported files and unverifiable fonts. Unsupported cases retain
canonical reading. Folder snapshot activation, raw-position reopening and USB
revalidation are implemented locally but have not been behaviorally tested.

The layout-only source map may represent one normalized space with one byte of
its original ASCII whitespace gap, including a tab or newline. The raw mapper,
not that alias, supplies precise selection and saved-caret positions.

Raw carets carry the canonical anchor, exact raw byte position, affinity and
source fingerprint. Mappings are checked against the open source and grapheme
boundaries. Canonical anchors resolve into raw-rendered text only when both
boundary affinities prove the same raw position; ambiguous or stale positions
fail closed. Raw pagination retains blank lines, and selection, copying,
search and resume use the verified raw positions. Unsupported foreign-note
projections and unresolvable selections are rejected. Cross-section quote
validation may reopen chapters and has not been performance tested.

Search workers rebuild the chapter layout under the storage lease and compare
its book/font stamps and raw source with the foreground layout. Prepared layouts
carry their verified stamps. Changed inputs or a missing required source mapping
reject the search; layouts that consistently lack stamps use canonical text.
Folder workers use the same immutable members for source and layout, and
revalidate their directory inventory and reading fonts at handoff and after USB
storage returns. These checks are not an atomic compare-and-swap against
non-cooperating external writers.

Raw-bearing state is saved in version 2. Before promoting an existing v1
library, Reader verifies the last loaded disk baseline, rejects data that would
be normalized or lost, retains an exact digest-named v1 backup, and publishes
the checked v2 state through the recovery journal. The journal verifies both its
captured old state and retained backup before commit. If a fresh installation has
no state file, the journal must prove the old slot was absent; no backup is
invented. Restore and preliminary saves apply the same checks. Pending recovery
blocks saving stale in-memory state. Older Reader versions reject v2 state.
Filesystems without directory sync do not provide a power-loss durability
claim; unrelated external writers are outside the supported lease model.

V2 library decoding rejects duplicate fields, unknown raw-caret placements,
invalid positions and lossy normalizations, and retains missing books. Backup
merges are conservative: conflicts reject the merge instead of dropping marks
or settings. Metadata sidecars and dated archives use strict single-book v2
records when needed; ordinary records retain v1 formatting. A dated archive
upgrade retains a digest-named v1 copy and rejects lossy upgrades. Older Reader
versions cannot read v2 sidecars or archives. Archive moves retain both source
and staged copies when journal recovery is uncertain.

The saved-mark browser verifies raw locations or the complete saved quotation
before direct note editing and reviewed bulk changes. Page overview can show
cross-chapter raw highlight cues after verifying the whole quote; work is capped
at four highlights of up to eight chapters per draw, so other cues are omitted.
For split ODT and large-text books, current-section raw anchors are kept only
after the legacy section check; raw anchors requiring translation from an old
unsplit source remain unsupported.

Stock XPointer annotations cannot represent positions inside raw whitespace.
Import preserves local raw marks, reports them as unavailable and disables
remote deletion cutoffs. Export reports omitted raw marks as unavailable; the
existing completeness guard refuses to publish an incomplete replacement.

[Pinned CRE whitespace formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2569-L2575)

Whitespace-only `<pre>` blocks use a separate bounded source collection and
virtual raw positions, without changing canonical block indices, existing
anchors or navigation. Their separate budget is up to 1 MiB per chapter.
The local implementation renders eligible reflow blocks under simple, transparent
block containers, with raw page anchors, selection and search. Blocks in tables,
floating layouts, in-page notes or unsupported box styles retain the previous
canonical fallback. Current virtual
positions in split ODT/text sections pass the current-section identity check;
legacy raw positions that would require translation remain unsupported.
This path remains untested.

When a whitespace-only raw line has no exact glyph cluster, a touch within a
single verified line maps to its exact start or end according to the touched
half of the column. Overlapping or unverifiable line regions are rejected.
This interaction remains untested.
