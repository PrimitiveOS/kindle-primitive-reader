# Large plain-text books

Reader accepts `.txt`, `.log`, `.txt.zip` and `.log.zip` up to 2 MiB of
encoded input and, for ZIP, 2 MiB of expanded text. Generated XHTML is bounded
to 4 MiB and may reach that limit earlier when text requires XML escaping.
Markdown, HTML, XML and TCR retain their existing limits. Large text files
whose contents are actually HTML are not supported by this path.

The large-text path also applies when at most 512 KiB of encoded plain text
expands beyond 512 KiB after decoding to UTF-8; this can occur with single-byte
encodings. The encoded and expanded-input limits above still apply. The
existing HTML-in-TXT detection and conversion remain unchanged. When the
extracted normalized text fits the earlier 512 KiB chapter limit, the
original converter, chapter path, publication revision and saved anchors are
retained. Oversized text instead uses the verified section mapping below.

For larger documents, Reader keeps its existing encoding, paragraph,
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

The decoded-size routing change is an implementation candidate; behavioral,
simulation, performance and Kindle checks remain open. Very long inputs may still hit
node, section, line, rendering or memory bounds and report an error. They do
not yet match KOReader's broader encoding and paragraph-detection behavior.
Standalone loose-document decoding recognizes BOM-marked UTF-32 little- and
big-endian input as well as the previously supported encodings. It rejects
incomplete code units, invalid Unicode scalars and NUL characters. BOM-less
UTF-32 detection is not implemented. Previously readable encodings retain
their conversion path and saved identities. UTF-32 behavior and real-book
compatibility have not yet been tested in simulation or on Kindle.
[KOReader text reader](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L1164-L1245) ·
[KOReader text import modes](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L2398-L2412)
