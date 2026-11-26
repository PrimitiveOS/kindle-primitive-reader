# Markup-heavy EPUB chapters

Reflow EPUB spine XHTML and HTML can now use up to 1 MiB of encoded member
bytes and 1 MiB of decoded or recovered XML. This helps with markup-heavy
chapters whose reading text still fits the existing 512 KiB text limit. The
XML parser's node limit and the existing image, font and layout budgets still
apply. Package and container metadata, navigation documents and fixed-layout
EPUB XML retain their 512 KiB limit; static SVG has its separate smaller limit.
Text-heavy chapters above the layout text limit still fail clearly and are not
split automatically.

The chapter's original archive path, content, source revision, spine index and
saved anchors are unchanged for already readable chapters. Normal reading,
search, CFI and KOReader-pointer paths obtain reflow content through the same
bounded chapter reader. Navigation group-target inference retains its 512 KiB
per-chapter inspection limit; it may leave a grouping row unavailable for a
larger chapter. Progress weighting still caps each chapter's contribution at
512 KiB, so it can underweight larger chapters. Optional eager preloading,
quote-prefix preparation and cross-part note discovery retain their independent
budgets and can defer work, fall back or decline the operation. The KOReader
pointer import's 512 KiB budget applies to linked stylesheets, not chapter text.

The larger memory bound is an implementation candidate, not a device acceptance
result. Decoder expansion, peak memory, real EPUBs, simulator behavior and
Kindle performance remain unverified.
