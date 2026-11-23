# Semantic type selectors in publisher styles

Publisher footnote sheets commonly use `aside[epub|type~="footnote"] { display: none }`.
The staged selector adapter retains the original element's local `type` value as
metadata and supports presence, equality and space-delimited word matching. An
optional `i` flag folds the compared value; original EPUB type tokens in the
acceptance case are ASCII. This metadata never changes canonical text, block
keys, revisions or stored source positions.

The pinned CREngine deliberately ignores namespace prefixes for selectors. Its
`parse_ident(..., true)` accepts `epub|type`, `*|type`, `|type` and unprefixed
`type`; `@namespace` is ignored. This adapter follows that provider contract,
including unknown prefixes. It does not advertise standards namespace binding.
The word operator uses literal U+0020 boundaries, as pinned CRE does; literal XML
attribute whitespace is normalized by the XML parser, while character-reference
tabs remain distinct.

Reference: [prefix parsing, lvstsheet.cpp:424](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/src/lvstsheet.cpp#L424),
[namespace policy:2598](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/src/lvstsheet.cpp#L2598),
[attribute matching:6082](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/src/lvstsheet.cpp#L6082).

These rules share the existing cascade and immutable flow view used by layout,
search and copy. A hidden note stays in canonical source but leaves the main
reading flow. Extracted popups omit the linked stylesheet and retain their own
inline attributes. Existing `[href]` presence matching remains supported. Other
attribute names, substring/dash operators, full non-ASCII CRE case-table parity
and namespace-collision precedence remain separate work. Strict user CSS still
rejects display/visibility until the remaining consumer gates are resolved.

Four authored ordinary tests cover selector matching and an actual rendered
main-flow/popup comparison. A separate opt-in check reads the unchanged owner
EPUB 034 and verifies its real semantic note rule through the shared font-free
cascade. Source `8572f35` passes 833 library tests on Mac and the full GitHub
workspace suite (833 library, 14 package and four system cases). All four focused
ordinary tests pass. The unchanged owner EPUB check finds seven semantic notes
and 1,175 intentionally omitted source bytes through its actual publisher rule.
This original check validates the computed rule, while the authored case paints
the actual reading and popup paths. Full original034 rendered-spine acceptance
after this selector change remains a separate combined replay gate.

Private source/binary hashes and logs are retained at
`build/parity-review/semantic-type-evidence/8572f35/proof.json`. No Kindle access,
package replacement or Store release occurs in this source slice.
