# Generic ZIP reading candidate

Reader's candidate generic ZIP adapter accepts one unambiguous TXT, LOG, MD,
RTF, FB2, HTML, HTM, XHTML, XHT or XML document inside a ZIP. Archives with
multiple readable documents remain rejected except for the bounded default
HTML case below; known unsupported book formats are rejected rather than guessed.
Images, CSS and other non-document assets do not count as separate books.
Archive entries are read in memory; no files are extracted. HTML and Markdown
reuse their bounded archive resource paths.

The candidate caps the archive at 16 MiB and 64 entries, validates the central
directory, rejects duplicate or unsafe paths, symlinks, encryption and
unsupported compression, and applies each existing per-format input limit.
Plain text is restricted to one 512 KiB decoded section in this first scope.
The caller will fingerprint the original archive under a distinct ZIP format
tag; the selected document's converted source remains the reading anchor.

XML members are limited to 512 KiB both before and after decoding. As for
standalone XML, a FictionBook root uses the strict FB2 reader, an HTML root
uses the XML-compatible HTML reader, and other XML is shown as source text.
External resources referenced from XML members are not loaded. An XML member
counts as a document for ambiguity checks, so a ZIP with both XML and another
readable document is rejected rather than choosing one. Previously supported
unambiguous ZIPs retain their existing reading path and identity. The XML ZIP
candidate has not yet been compared with KOReader or behaviorally verified.

For a formerly rejected multi-document archive, Reader can select a single
unambiguous root `index.html`, `index.htm`, `default.html` or `default.htm`.
The selection is case-insensitive, rejects duplicate document paths and multiple
defaults, and is unavailable if an XML or unsupported book member is present.
The existing HTML archive selection and resource bounds still apply. Nested
defaults and broader multi-document selection remain unsupported; this
fallback has not yet been behaviorally verified.

KOReader registers generic ZIP and CREngine can choose among more archive
members, preferring default HTML. Reader requires one supported document or the
unique root default described above, and does not yet open PDF, EPUB or office documents nested
in generic ZIP. [KOReader registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1510) · [CREngine selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4501-L4553)

Reader's candidate Library integration identifies a generic ZIP from its
original archive bytes under a distinct ZIP tag and uses the selected member's
actual reading format. Files has a paired handoff change; neither update should
be deployed alone. Source identity, ambiguity, malformed archives, actual
Files handoff, simulator and Kindle behavior have not been verified.
