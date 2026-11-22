# CSS display and visibility: consumer migration

This phase connects the canonical-source-preserving projection to horizontal and
vertical reflow layout, hit testing, selection, word/context lookup and session
restoration. Publisher `display:none` omits layout/search/copy text;
`visibility:hidden` and `collapse` retain layout/search/selection while removing
that element's paint. A visible descendant may restore paint. Canonical source
text, revision, fragment tuples and saved quote content remain unchanged.

The strict user-CSS parser still rejects `display` and `visibility`. There are no
hiding presets or a claim of complete CSS box-model support. The unresolved gates
below must be met before exposing that interface.

## Shared view and stable anchors

`ChapterFlow` binds each affected block's shared `Arc<OwnedProjection>` to the
same immutable parsed source. Layout-only `LayoutBlock` inputs borrow canonical
images and ruby records, with projected text, style spans and media slots. Empty
identity paths do not duplicate canonical text. Parser lifetime/revision plus the
immutable active style snapshot are required cache keys; owned projections cannot
be rebound to a separate parse.

Parley and vertical glyph clusters use projected bytes. Persisted anchors remain
canonical UTF-8 source offsets. `Line.spans` records actual retained canonical
intervals, so a line that joins two surviving words across removed text does not
count or highlight the hidden envelope. Search/nearby workers use the shared
font-free chapter flow and retain canonical disjoint hit intervals (see
[search migration](css-flow-search.md)). The app revalidates the immutable search
configuration before activating a result.

New copy, Unicode word expansion, CJK scan, dictionary context and snippets use
the rendered view. Historical annotation recovery uses `select_source_range` and
canonical source wording; changing publisher CSS must not falsely mark an old
unchanged quote as corrupt. Highlight geometry still intersects only the retained
projected spans. Combining marks and bidi text are tested after hidden inline
content joins the surviving text.

## Layout and navigation

- Text, ruby, media, list markers and background paint use separate computed
  suppression and painting masks. `display:none` images are not decoded; visibility-hidden
  media keeps its dimensions. A removed `rt` has no annotation layout, while an
  unpainted `rt` keeps its ruby geometry.
- Markers retain the list item's own visibility, independently of a visible child
  paragraph. Their displayed counters come from participating computed list
  items; canonical ordinals remain unchanged. A marker moves to the first retained
  paragraph, or keeps an empty marker strut if all its child content is removed.
- Tables, borders, width measurement and padding share the same validated table
  plan. The next retained block decides a cell's end, rather than a skipped hidden
  block. Unsupported changes to specified fractional column hints use the
  narrowly typed readable source-order fallback and report a limitation; they
  never count as grid fidelity. Other layout/parser errors propagate.
- Illustration anchors retain the original image ordinal even when an earlier
  image is removed or canonical text becomes a pure-image display block.
- An explicitly hidden fragment owner is unavailable. A visible container whose
  first text was removed can resolve to retained content inside its own subtree;
  it cannot silently select a following unrelated passage.
- Automatic restore may snap the displayed page to retained content while keeping
  the original valid canonical focus. Reflow, scroll/spread changes and reopening
  retain that focus; actual reading motion clears it. Explicit bookmark/link jumps
  never use this fallback.
- Saved user flow markers retain their source JSON and re-resolve CSS-omitted
  marker text against current pagination. This is boundary resolution, not an
  instruction to reveal the hidden glyph. Synthetic character-page numbering
  remains canonical as in pinned CRE.

## Verification API

`Probe::source_presentation` classifies a source anchor as `Normal`,
`VisibilityHidden` or `CssOmitted`; `fragment_presentation` classifies its actual
DOM owner. Both report intended CSS treatment, not successful layout.
`layout_limitations` reports readable degraded layouts separately.

`coverage()` retains **all canonical non-whitespace characters** in its
denominator and counts only represented retained intervals. A verifier must keep
intentional omissions separate from unexpected unmapped normal or
visibility-hidden content. A visible container with a hidden prefix remains an
eligible fragment even when its original caret points into omitted bytes.

## Authored desktop evidence

The test fixtures exercise the actual reflow engine. They are deliberately small
and contain no copied owner-book content. These pages show the retained text,
paint-only gap with a visible child, vertical paint-only gap and explicit hinted
grid fallback.

| Display omission | Visibility without collapsing space |
| --- | --- |
| ![Projected document](screenshots/css-display-projected.png) | ![Hidden paint and visible child](screenshots/css-visibility-hidden.png) |

| Vertical projection | Reported source-order table fallback |
| --- | --- |
| ![Vertical text](screenshots/css-vertical-hidden.png) | ![Source-order table](screenshots/css-hinted-table-source-order.png) |

Focused tests cover 17 provenance, 6 shared-view, 3 existing vertical, 8 Probe,
2 live session/custom-boundary and 1 marker cases (37 distinct passing focused
cases). The workspace builds successfully on the Mac. The final serial library suite
passed 813 tests with 62 opt-in cases ignored; package 14 and system 4 suites also
passed (831 ordinary tests, zero failures). The authorized original 034 projection
and independent extracted-popup source contract also passed with its unchanged
SHA-256; this does not establish namespace-selector or rendered-spine parity.
Exact Mach-O executable hashes, source hashes, build JSON and screenshots are
retained privately under `build/parity-review/flow-consumer-evidence/`.

## Combined desktop checkpoint

The assembled source `9a5006e2` includes these consumers, immutable styled search,
the four-case footnote-root correction, custom content DPI and explicit verifier
intent accounting. It passes 829 library, 14 package and four system tests on
Mac (847 ordinary cases). Eight original-document defaults, complete-backup and
DPI runs also pass with unchanged EPUB/PDF/AZW3/MOBI hashes. The verifier keeps
all canonical characters in its denominator and reports CSS omissions,
visibility-hidden geometry, fragment-owner intent and layout limitations
separately. Losing visibility-hidden geometry remains a failure.

The `9a5006e2` replay exposed three rejected media-only lines in original EPUB
053. The subsequent `e79c7afe` aggregate repairs them and passes 862 ordinary Mac
tests. It opens/resumes all 48 unchanged editions and checks all 1,509 sections,
28,071 frames and 28,061 inspected page anchors. All 26,591,258 retained source
characters map; publisher CSS intentionally omits 797 out of the unchanged
26,592,055-character denominator. Its fragment audit leaves one eligible target
in original039 chapter7, alongside eight intentional omissions.

The subsequent [generated-target component](generated-boundary-targets.md)
resolves all 38 eligible destinations in that original chapter and passes 28
save/reopen/reflow cases across ten unchanged editions. Integration with paired
spacing and the actual resumed-session verifier still needs its next aggregate.
These are desktop results; the official package remains 0.1.22.

## Remaining acceptance gates

1. A generated-only visible LI/container with all source children removed needs a
   durable DOM/block-boundary anchor and box geometry. Its source intent remains
   eligible; it must never be relabeled an intentional omission to hide the
   missing geometry. The bounded horizontal marker/empty-box/inline-caret path
   now has component verification; unsupported table/vertical/fixed-height-only
   geometry and nested custom-flow ordering remain listed separately.
2. [Source-aware popup roots](footnote-export-roots.md) pass eight native-oracle
   cases, nine Mac checks, 16 authored captures and seven actual note links from
   unchanged original034. Arbitrary CSS box reclassification and anonymous
   final-box ownership remain conservative refusals, with broader native cases
   outside this bounded slice.
3. Arbitrary non-`none` display box reclassification, broader attribute selectors
   and exact changed fractional grid fidelity are not claimed. The supported
   [semantic-type selector](semantic-type-selectors.md) matches original034's
   actual note rule. The table fallback remains readable but degraded.
4. Partial affected provenance that exceeds the bounded complete map is rejected
   explicitly; unaffected/identity chapters and fully suppressed blocks remain
   usable. Same-text-node whitespace compaction avoids exhausting the event cap
   for ordinary long paragraphs.
5. The combined whole-spine original-document verifier must separately account
   for intentional CSS omissions, retained unpainted content and real failures.
   The earlier 48-original search receipt is not full-layout evidence.
6. Combined aggregate/native compilation, all-original rendered-spine replay and
   connected Kindle storage/paint/input acceptance remain parent integration
   gates. This phase performs no device calls, installation or release.
