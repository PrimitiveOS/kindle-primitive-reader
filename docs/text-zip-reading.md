# Compressed plain-text books

Reader opens `.txt.zip` and `.log.zip` files containing exactly one matching
`.txt` or `.log` document through its existing plain-text reader. It also
opens a `.md.zip` file containing exactly one `.md` document through its
Markdown reader. The title derived from the filename omits both
extensions. TXT and LOG archives and their expanded document are each limited
to 2 MiB; Markdown ZIP retains its 512 KiB limit. No file is extracted to
disk. The reader rejects more than
64 entries, unsafe paths, links, encrypted or unsupported entries, and archives
with no matching document or multiple matching documents.

For `.md.zip`, directly referenced images beneath the selected Markdown
member's directory are read from the same validated archive, with at most 64
references, 2 MiB per image and 8 MiB total. Missing, unsafe, ambiguous,
oversized or unreadable images use the normal placeholder. The Markdown XHTML,
book identity and saved anchors are unchanged; image loading can repaginate.
Parent-directory references and resources referenced only inside SVG remain
unsupported, and files beside the archive are never used.

KOReader registers `.txt.zip` with CREngine. Its general archive importer can
choose other document types and prefers a default HTML document; Reader's
wrapper deliberately accepts one text file and leaves those mixed archives
outside this feature. [KOReader registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1620-L1622) · [CREngine archive selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4712-L4793)

The larger TXT/LOG path remains unverified; see [large plain-text books](large-text-reading.md)
for conversion, section and anchor limits. Archive validation, encoding,
page turns and resume need simulator and device verification before release.
