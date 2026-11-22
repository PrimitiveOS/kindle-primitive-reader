# Footnote export roots

Opening a footnote now computes its export root from the original chapter's supported paragraph flow. It uses the same font-free publisher stylesheet cascade and immutable user-style snapshot as the reading engine. A hidden target does not by itself stop the search: a higher visible paragraph may still own the final text block. If no supported final owner exists, the original target is exported. Semantic footnote recognition remains independent of root selection.

The adapter borrows canonical `Source` metadata, checks its exact raw-input SHA-256 fingerprint, and starts at the fragment's true element owner. The input fingerprint is separate from versioned Ruby/numeric text-model revisions and is bound after those parse stages retain their existing canonical values. It verifies the complete raw tag and sibling-ordinal ancestry before selecting an export slice. This prevents a duplicate ID, an anonymous wrapper or a variable-length numeric-reference conversion from binding the target to a different element's styles. It does not rewrite the source, canonical text, saved anchors or fragment tuples.

`display:none` suppresses a final owner. `visibility:hidden` preserves final ownership because it changes painting rather than paragraph participation. The selected raw XML slice retains its inline styles. Linked stylesheets are outside the separate popup stylesheet context, as in the pinned KOReader export path. Publisher-style policy is honored when choosing the original chapter's root; popup typography remains independently configured.

## Native reference

The reference is KOReader `9192014d8bd82a91dc1012473be0f238dedfdb54`, base `6e4bc81af4e04f78d81677a323a780a71b29702a`, CREngine `b32a88ffbbf505cf2ea5208c1a49002d70d3609f`. The native implementation walks target-to-root inclusively for the first actual `erm_final` node; absent one, it serializes the original target at offset zero. This is distinct from the preceding footnote classification and optional extended-range decision. [Native export](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L2785) · [Final-node search](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L9983) · [Popup policy](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L1408)

The byte-identical [authored EPUB](../tests/fixtures/footnote-export-roots.epub) was laid out in the actual stock Mac SDL runtime in run `footnote-oracle-20261122T095415Z`. Ordinary geometry taps invoked `ReaderLink:onTap`, native classification with flags `0xdf7e`, `getHTMLFromXPointer(target,0x1001,true)`, and genuine `FootnoteWidget` construction. All eight kept the source position. No PNG or owner document was used in this oracle, and the frozen runtime and fixture hashes matched before and after. This establishes extraction/controller behavior on Mac, not Kindle acceptance or identical font rasterization.

| Case | Source context | Native export root |
| --- | --- | --- |
| A | ID on P inside an inline-hidden semantic aside | P; ancestor aside/style omitted |
| B | ID on the inline-hidden aside | Aside with its inline `display:none` |
| C | ID on P inside a semantic aside hidden by linked CSS | P |
| D | ID on the linked-hidden aside | Aside and class; linked stylesheet omitted |
| E | ID on a visible span in a visible P | Whole P |
| F | ID on span in P inside a hidden block div | Original span only |
| G | ID on em inside hidden inline span in otherwise visible P | Whole P, retaining the hidden span/style |
| H | ID on visible span in div containing inline flow | Whole div |

Fixture SHA-256: `84fff6bd39157018dd1c3a2495996bf6ee95a3cd6edcc1b955612f9874a9b981`. Private local receipt SHA-256: `8896dec6f33731e5bc87088ac73ed965bcb26e97b29e11d6a2c11f91044092a1`. The fixture and independently authored tests use the repository licence; no stock implementation or GPL/AGPL rule dataset is bundled in Reader.

## Bounds and verification

This implements supported native paragraph/inline-flow ownership using canonical block owners and computed suppression. It is not a complete CREngine box classifier. Affected arbitrary CSS display reclassification and anonymous final boxes decline a popup and leave the ordinary link workflow available. Empty anchors between blocks require a separate extended-range policy; the standalone legacy extraction tests retain their existing fallback independently. Complex tables, floats, cloned render boxes and other engine-specific classification remain separate acceptance work. No inline style is stripped to make a hidden target appear.

The deterministic tests compare all eight roots, linked CSS loading, publisher policy, visibility versus display, duplicate/stale identities, numeric references combined with ruby, unsupported boxes and unchanged canonical positions. The application test taps every source link, checks the actual popup render and hidden-text treatment, closes normally, compares the original page pixels and position, then reopens the saved book. Optional `READER_FOOTNOTE_ROOT_SCREENS` writes light/dark screenshots for visual review. [Root tests](../src/footnotes/layout_roots/tests.rs) · [Popup test](../src/app/footnote_popup/tests.rs)

[Remote CI](https://github.com/PrimitiveOS/kindle-primitive-reader/actions/runs/45012174729) passed at source commit `f08430e9402630498b6602f56aa2cfe1734f8bdc`: 828 library, 14 package and four system tests; 62 opt-in cases were omitted. All eight layout-root test functions and the application popup test passed. This includes the versioned Ruby/numeric source model and exact raw-input binding, with the actual fixture's linked CSS loaded. Local compilation, screenshot review and native Kindle acceptance remain pending until their exact candidate artifacts are tested.
