# Legacy MOBI text destinations

Reader can resolve legacy MOBI `filepos` links and contents entries pointing
inside ordinary text in a small, single-section legacy publication. The
source coordinates refer to the original decompressed byte stream: UTF-8
targets must fall on a character boundary, and Windows-1252 uses its original
single-byte positions. Targets in tags, comments, character references,
ignored raw-text bodies, or a CRLF pair remain unavailable.

To preserve existing reading positions, Reader never inserts the new anchors
or link attributes into the actual chapter. It builds a temporary shadow from
the same normalized chapter and uses the existing section assembly. It then
projects link ranges and destinations only if the shadow and actual Source have
matching ordered block text, image identities and offsets, and element
ancestry. Existing links, canonical XHTML, source revisions, block keys and
stored anchors stay unchanged. A failed or ambiguous projection leaves the
link unavailable. The shadow is limited to a 512 KiB normalized chapter and
1 MiB generated markup. Cross-section and image-only links are not yet added.

KOReader registers MOBI with CREngine and opens the PDB text stream with its
HTML parser. The inspected source establishes the reference import path; it
does not establish KOReader's behavior for these specific `filepos` cases.
[KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1611) · [CREngine PDB import](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L1282-L1331)

This change has not yet been built or tested. It needs original-book and
simulator comparisons, including saved bookmarks and highlights, before a
compatibility claim or device deployment.
