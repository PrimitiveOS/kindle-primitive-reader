# Legacy HTML named anchors

Older XHTML and HTML books may use `<a name="section">` as a link target.
Reader now indexes unique legacy names through the same text-position and
visibility machinery used for element IDs. Contents and inline links can then
resolve those targets without changing the source text, source revision or
saved anchor tuples.

Existing publisher IDs and generated Kindle destinations take precedence.
Names that are duplicated, collide with an ID, contain control characters,
exceed 512 bytes, or use the reserved MOBI shadow prefix are not indexed.
The scan is limited to 8,192 names or IDs and 128 KiB of combined attribute
values; exceeding the scan budget disables this fallback for the chapter so
that a later collision cannot silently change an existing target. Hidden or
otherwise unrenderable targets retain the existing visibility checks.

This candidate has not yet been behaviorally compared with KOReader or tested
with real books, the simulator or a Kindle. Other obsolete HTML anchor forms
and ambiguous duplicate names remain unsupported.
