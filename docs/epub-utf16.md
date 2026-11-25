# EPUB XML encoded as UTF-16

Reader can decode BOM-marked UTF-16LE and UTF-16BE EPUB container, package,
navigation and chapter XML through the existing bounded XML reader. Each
archive entry and its decoded UTF-8 representation remain limited to 512 KiB;
invalid code units, NULs and XML encoding declarations conflicting with the
BOM are rejected. A declaration, when present, is validated and limited to
1,024 decoded bytes. The resulting text goes through the existing EPUB XML
preparation and DTD-disabled parsing rules.

Previously readable UTF-8 entries keep their original text, package revision
and source anchors. The new path does not detect BOM-less UTF-16, UTF-32 or
other non-UTF-8 encodings. These additions have not yet been behaviorally
compared with KOReader or verified with real books, in the simulator or on
Kindle. [Pinned CREngine encoding detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L6108-L6122)
