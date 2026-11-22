# Durable generated-box targets

Source-only implementation checkpoint; tests have been authored but are not yet
compile/runtime verified. No strict user CSS hiding syntax is enabled.

`Anchor.boundary` is optional, defaulted and omitted from legacy JSON. It contains
an immutable normalized-XML owner byte origin, plus an optional bounded integer
relative position (0–10000) for later pages inside a generated box. It stores no
parsed element index, fake text or replacement source key. Resolution requires
that exact current source revision, an actual non-selector/non-anonymous owner
and its unchanged canonical fallback pair. Text selection and highlight endpoints
reject boundary targets.

The horizontal renderer retains real LI marker geometry. It also lays out empty
native padded/bordered block containers outside tables/lists, including nested
containers and bare empty containers that have a surrounding canonical fallback.
Background and stroke rendering share existing bounded primitives. Source-order
empty named owners between blocks get precise zero-height DOM boundary targets,
so their canonical tuple cannot alias a following paragraph's text. Valid empty
inline caret targets within a retained paragraph keep their previous caret path.

There are at most512 nonzero empty boxes and4096 empty named-point owners per
chapter; nesting uses the existing64-level source cap. Nonzero boxes consume
bounded real vertical space and can span pages. Only later generated-only page
anchors use a relative position; fragment jumps target the owner start. Reflow or
reopen keeps the exact source owner, and a later style reveal resolves inside
that same owner's retained content.

Remaining unsupported geometry includes independent empty native-table cells,
list decorations other than the existing LI marker path, inline empty targets
inside wholly omitted paragraph content, arbitrary display reclassification,
fixed-height-only empty boxes and vertical-writing generated boxes. A chapter
with no canonical source block at all cannot create a fallback-key boundary.
These eligible targets must remain visible verifier failures/limitations rather
than being called CSS omissions. Source bytes/text/keys/revisions/fragments and
legacy anchor serialization are not changed by this metadata.
