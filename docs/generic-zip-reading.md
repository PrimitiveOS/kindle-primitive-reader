# Generic ZIP reading candidate

Reader's candidate generic ZIP adapter accepts one unambiguous TXT, LOG, MD,
RTF, FB2, HTML, HTM, XHTML or XHT document inside a ZIP. It rejects multiple
readable documents and known unsupported book formats rather than guessing.
Images, CSS and other non-document assets do not count as separate books.
Archive entries are read in memory; no files are extracted. HTML and Markdown
reuse their bounded archive resource paths.

The candidate caps the archive at 16 MiB and 64 entries, validates the central
directory, rejects duplicate or unsafe paths, symlinks, encryption and
unsupported compression, and applies each existing per-format input limit.
Plain text is restricted to one 512 KiB decoded section in this first scope.
The caller will fingerprint the original archive under a distinct ZIP format
tag; the selected document's converted source remains the reading anchor.

KOReader registers generic ZIP and CREngine can choose among more archive
members, preferring default HTML. Reader deliberately requires exactly one
supported document and does not yet open PDF, EPUB or office documents nested
in generic ZIP. [KOReader registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1510) · [CREngine selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4501-L4553)

**Integration pending:** the adapter is not yet connected to Reader's Library
format/identity or its open path. The paired Files handoff must not be deployed
alone. Source identity, ambiguity, malformed archives, actual Files handoff,
simulator and Kindle behavior have not been verified.
