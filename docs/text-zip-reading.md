# Compressed plain-text books

Reader opens `.txt.zip` and `.log.zip` files containing exactly one matching
`.txt` or `.log` document through its existing plain-text reader. It also
opens a `.md.zip` file containing exactly one `.md` document through its
Markdown reader. The title derived from the filename omits both
extensions. The archive and expanded document each use the existing 512 KiB
plain-text limit; no file is extracted to disk. The reader rejects more than
64 entries, unsafe paths, links, encrypted or unsupported entries, and archives
with no matching document or multiple matching documents.

KOReader registers `.txt.zip` with CREngine. Its general archive importer can
choose other document types and prefers a default HTML document; Reader's
wrapper deliberately accepts one text file and leaves those mixed archives
outside this feature. [KOReader registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1620-L1622) · [CREngine archive selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4712-L4793)

This change has not yet been built or tested. Archive validation, encoding,
page turns and resume need simulator and device verification before release.
