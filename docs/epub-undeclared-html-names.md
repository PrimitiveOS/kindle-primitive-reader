# Undeclared HTML character names in EPUB chapters

Some reflowable EPUB chapters omit an HTML doctype while using built-in HTML
character names such as `&nbsp;` or `&mdash;`. XML requires those names to be
declared, so Reader previously could not parse such a chapter. The local
candidate retries only an archive-backed reflow chapter whose strict XML parse
reports an unknown entity. It replaces supported built-in HTML names with
equivalent numeric references and accepts the result only if the whole chapter
parses as bounded HTML-root XML. Text, attributes, markup and fragment IDs remain
in place; comments, CDATA and processing instructions retain their literals.

The retry refuses original declarations and custom entities, unresolved names,
malformed markup, oversized output and fixed-layout pages. Declaration detection
is intentionally conservative and can decline a document containing declaration
text in a comment. The publisher's archive stays unchanged. A chapter that
already parsed keeps its exact previous XHTML and source revision; the newly
readable chapter uses deterministic normalized XHTML for its reading anchors.
No general HTML tag repair, external DTD access, custom entity expansion or
conversion of package metadata is implied. Chapter consumers including local
links, note lookups, CFIs and synthetic page indexing read through the same
chapter path. Navigation-group fragment checks also use it for uniquely
identified reflow spine items. A separate malformed navigation document is not
recovered during ordinary navigation loading. If Contents or the page list is
still empty after the existing nav and NCX paths, Reader separately retries an
HTML navigation document with the same bounded entity recovery. Existing
nonempty lists take precedence, and recovered group headings use the same unique
spine and fragment checks as normal navigation. Other malformed navigation
documents and OPF/NCX files remain outside this recovery. Link and fragment
behavior for newly recovered documents, memory limits, real EPUBs, comparison
with KOReader, simulator behavior and Kindle acceptance remain unverified.
