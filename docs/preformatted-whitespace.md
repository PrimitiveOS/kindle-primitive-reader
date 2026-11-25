# Preformatted whitespace

The local Reader build enables a bounded horizontal layout for simple source
blocks with `white-space: pre`, `pre-wrap` or `break-spaces`. It preserves
verified spaces, tabs and line breaks while keeping existing canonical source
identities. This implementation has not yet been behaviorally tested in the
simulator or on Kindle and must not be treated as validated or released.

A snapshot is captured for direct text and a bounded set of inline elements
when the concatenated text normalizes to the canonical block exactly. Inline
style runs retain each source text node’s owner, including whitespace at
inline boundaries; each run keeps its computed style and language. Explicit
`<br>` elements map to verified canonical newlines. Comments and processing instructions are ignored. Each block is limited to
64 KiB and 4,096 mapping records; chapter metadata budgets are 768 KiB for
preferred `<pre>` content and 256 KiB for other content. The ordinary layout
is used for content outside these bounds, unsupported projections, images,
ruby, generated content, unsupported inline styles, transforms, bidi controls, joined
notes and in-page footnotes. A layout whose source or ordered font stack cannot
be stamped also uses the canonical path. Folder books retain canonical search.

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
The stamps are not an atomic compare-and-swap against non-cooperating external
writers.

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
