# Display and visibility: cascade foundation

This source-only phase adds computed flow metadata. It does not hide content in
Reader yet and does not enable these properties in custom user CSS. Rendering,
search, selection, links, contents, artwork and counters must adopt the same
canonical projection before the feature can be offered.

The pinned sources are KOReader `9192014d8bd82a91dc1012473be0f238dedfdb54`, base
`6e4bc81af4e04f78d81677a323a780a71b29702a`, and CREngine
`b32a88ffbbf505cf2ea5208c1a49002d70d3609f`.

## Computed contract

`Style` exposes `display`, `visibility` and the final `flow: ElementFlow`:

- `suppressed = parent.suppressed || display == None`. A descendant cannot
  restore content suppressed by an ancestor's display.
- `painted = !suppressed && visibility == Visible`. Visibility is inherited;
  a child may explicitly restore visible painting within a hidden/collapsed
  ancestor while retaining ordinary layout space.
- Collapse is retained as a distinct value but paints like hidden, including
  table cells. This follows the pinned engine's documented limitation; it does
  not implement zero-space table-row/column collapse.
- Display defaults to the shapes already derived from native Source tags and is
  not normally inherited. The pinned engine explicitly does not implement
  display inheritance: `display:inherit`, `initial` and `unset` behave as inline.
  This limitation is deliberately preserved rather than silently replaced with
  different CSS-standard behavior. Visibility inherit/unset use the parent;
  visibility initial uses visible.
- The existing origin/importance tiers remain: main/user normal, author normal,
  author important, user important; inline priority/specificity/order apply
  inside their tiers. Inherited values do not inherit declaration importance.

Reference: `cssdef.h:27–47,346–350`, `lvstsheet.cpp:3646–3653,4808–4811`,
`lvrend.cpp:10253–10260,11278`.

Publisher declarations recognize the pinned bare display values: ruby, run-in,
inline, block, legacy `-cr-list-item-final`, list-item, inline-block, inline-table,
table and its row-group/header-group/footer-group/row/column-group/column/cell/
caption parts, and none. Flex, grid, contents, flow-root, multi-keyword display
syntax and arbitrary modern layout modes remain unsupported.

Recognizing a non-none value as metadata does not implement a box transformation.
`Display::requires_reclassification(tag)` reports a non-native requested shape.
The current renderers still derive their shapes from Source; consumers must not
claim that a span can become a table, flexbox or list solely because this enum
exists. Strict user parsing explicitly rejects both property names, even for
otherwise recognized values, until the complete consumer gate is opened.

## Shared loader

`BookStyle::load_for_flow(book, chapter, source, publisher_styles, snapshot)`
returns the same cascade inputs used by native rendering. Both paths call one
`load_sheets` implementation, sharing stylesheet selection, class/ID filtering,
byte/rule/sheet limits, inline-style handling and user-origin rules. The flow path
has no FontContext parameter and performs no family enumeration or font-face
resource registration. It may parse bounded face declarations as stylesheet
metadata; it does not load their font buffers. This avoids creating a publication
font collection merely to evaluate search/navigation masks.

The immutable, font-independent user snapshot remains separate from the
Publisher CSS switch. The current user parser gate also remains in effect when
that snapshot is passed to this loader.

## Rendered list ordinals

`book_style::flow::list_ordinals` accepts computed styles and direct-child
`ListChild { element, value }` metadata, plus the container's explicit start and
reversed flag. It returns a separate `Vec<Option<i32>>`; canonical source list
ordinals and text offsets are untouched.

Only computed list-item/legacy-list-item children that are not display-suppressed
participate. A suppressed explicit value neither resets nor increments the
counter. Visibility-hidden/collapsed items still participate. A markerless
list-style also participates. Reversed lists without an explicit start use the
participating count. Visible explicit values reset the running ordinal, even
when the value happened to equal the old natural ordinal. Invalid/duplicate
indices are rejected; arithmetic retains Reader's existing saturating i32 bounds.

Reference: CRE `lvtinydom.cpp:21114–21218`. Source metadata must retain explicit
values/start/reversed data rather than infer them from old ordinal differences.
Statically omitted HTML-hidden children are handled by Source's existing retained
node map. `selector_only` alone is not a safe general-purpose hidden-node flag.
The metadata producer and consumer are separate changes owned by the integration
phase.

## Proof obligations

Nine authored tests cover ancestor suppression and restored child visibility,
reset keywords including the pinned display-inherit limitation, origin/inline
importance, continued user-input rejection, recognized/unsupported display modes,
shared font-free/renderer masks and stylesheet caps, unchanged canonical source,
rendered list counting/reset/reverse cases, markerless/hidden participation and
invalid metadata. These tests are source-ready; no local Rust build or execution
has been performed for this phase during the isolated KO reference build.

Before integration: compile and run these cases plus the ordinary suite through
CI or the next coordinated host build. Before enabling user CSS: complete every
layout/search/navigation consumer, exercise real hidden-footnote/inline/artwork
cases and list metadata, preserve anchors/marks and all-hidden recovery, and pass
native/device gates. This phase makes no rendering, physical-display, release or
full CSS-parity claim.
