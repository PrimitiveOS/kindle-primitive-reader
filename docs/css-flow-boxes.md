# CSS flow: native table and publisher box helpers

This slice prepares the geometry helpers for the shared computed flow masks.
It does not enable user `display`/`visibility` declarations, replace canonical
source cells, or claim completed renderer integration. The consumer work must
construct one validated `table_layout::TableFlow` and pass it to the new
`*_with_flow` entry points. The older entry points retain an identity table plan
for existing callers; their box paint/spacing filters use the supplied masks.

`TableFlow::new(source, styles)` borrows the original cell slice without copying
on ordinary chapters and visibility-only changes. A transformed native grid
retains a sparse canonical-cell lookup, removes suppressed rows/cells, repacks
surviving cells with the existing 32-column occupancy limit, and preserves
colspan, RTL and the Reader's existing bounded rowspan duration. The duration
counts retained rows, rather than subtracting removed DOM rows. Canonical
`Source.table_cells`, each block's cell, text, block keys and anchors stay intact.
This is native table suppression; arbitrary `display` box reclassification,
nested-table layout and CRE's broader malformed-table repairs remain separate.

The pinned CRE source supports these distinctions:

- `lvrend.cpp:381–384` omits invisible table nodes before creating rows/cells;
  `483–495` reads spans and `672–689` places cells in that retained grid.
  CRE supports cases beyond Reader's existing HTML group-clamped span metadata;
  this change does not invent missing original attributes.
- `lvrend.cpp:835–920` resolves collapsed edges into the owning cell style;
  `10253–10260` then suppresses a visibility-hidden owner's own border/background
  while allowing visible descendants to paint. Hidden paint therefore does not
  erase border width or alter conflict selection.

These references are the local engine at
`b32a88ffbbf505cf2ea5208c1a49002d70d3609f`, paired with the pinned KOReader
reference. No engine source or style dataset is copied into this implementation.

Existing per-cell column edge fractions do not retain the original distinction
between specified and automatic `<col>` widths. If CSS removal changes those
columns, reconstructing exact widths would require new publisher metadata.
`TableFlowError::HintReprojection` identifies only that bounded limitation;
`TableFlow::source_order(source)` supplies an explicit grid-free readable plan
for the consumer to use while reporting degraded table layout. Other errors,
including a projected grid exceeding 32 columns or a mismatched style vector,
propagate. A table that is wholly suppressed needs no hint reconstruction.
The consumer owns the narrow fallback and its visible limitation reporting.

`Borders::read_with_flow` and `publisher_padding::layout_with_flow` share the same
projected cell identities. Removed cells cannot affect intrinsic column demands,
border spacing or cell padding. The measurement callback must shape projected
text; this helper does not copy/rewrite source blocks. Visibility-hidden cells
keep border widths, cell positions and spacing, but the final cell's strokes
and background do not paint. A hidden table's separate outer frame also stays
unpainted while visible descendants retain their own paint.

`vertical_padding::layout_with_flow` uses retained cells for grid-boundary
classification, including the explicit source-order fallback. Suppressed blocks
cannot claim a wrapper's first/last padding or background extent. Hidden
visibility keeps padding geometry and propagates that extent to ancestors;
only the owner's own background is suppressed. Publisher forced/avoid/keep
rules and widow/orphan protection omit suppressed blocks, including an empty
separator's following-block fallback. `BoxArea::paragraph_bounds_for_text`
resolves auto direction from projected text, so a removed leading directional
run cannot select the wrong side for a negative indent.

Twelve ordinary regressions are authored for identity allocation, retained-row
spans/colspans, one-column RTL, whole-table removal, typed hint fallback,
overflow refusal, removed-cell measurement, projected auto direction,
collapsed geometry versus paint, wrapper padding/fills and page-break rules.
Only source formatting/parsing and diff checks were performed locally during
the KOReader build resource reservation. CI execution, assembled renderer
screenshots, original-file workflows, native compilation and Kindle acceptance
are separate gates and must be recorded from their actual receipts.
