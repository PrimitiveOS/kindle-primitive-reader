# Large plain-text books

Reader accepts `.txt`, `.log`, `.txt.zip` and `.log.zip` up to 2 MiB of
encoded input and, for ZIP, 2 MiB of expanded text. Generated XHTML is bounded
to 4 MiB and may reach that limit earlier when text requires XML escaping.
Markdown, HTML, XML and TCR retain their existing limits. Large text files
whose contents are actually HTML are not supported by this path.

Documents already accepted by the earlier 512 KiB path keep the same converter,
chapter path, generated publication revision and saved anchors. For newly
accepted larger documents, Reader keeps its existing encoding, paragraph,
terminal-color and newline handling. It partitions oversized normalized text
into bounded reading sections and verifies the extracted source text and image
order against the original generated document before opening it. A paragraph
continued across sections is joined without adding a newline when copying or
verifying a highlight. Section boundaries are implementation details and can
appear in chapter-oriented navigation and footer counts.

Book identity is still derived from the original file and publication revision
from the complete generated XHTML. The source-verified section mapping checks
saved bookmarks, highlights, hidden flows and custom contents before a cloned
state can replace the old one. Unknown or inconsistent chapters, revisions,
blocks, offsets or page-only locations fail closed. Current-section anchors
are checked against that section's actual block text before use. A quote import
uses the same section text and is verified before saving. Quotes crossing an
artificial section boundary may not be found by the import search.

These changes are implementation candidates: no build, simulation, behavioral,
performance or Kindle tests have yet been run. Very long inputs may still hit
node, section, line, rendering or memory bounds and report an error. They do
not yet match KOReader's broader encoding and paragraph-detection behavior.
[KOReader text reader](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L1164-L1245) ·
[KOReader text import modes](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L2398-L2412)
