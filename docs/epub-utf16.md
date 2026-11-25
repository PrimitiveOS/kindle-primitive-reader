# EPUB XML text encodings

Reader can decode BOM-marked UTF-16LE and UTF-16BE EPUB container, package,
navigation and chapter XML through the existing bounded XML reader. It also
recognizes BOM-less UTF-16 only when the file starts with the exact UTF-16
byte-order signature for an XML declaration; that declaration must be present
and valid. Each
archive entry and its decoded UTF-8 representation remain limited to 512 KiB;
invalid code units, NULs and XML encoding declarations conflicting with the
BOM are rejected. A declaration, when present, is validated and limited to
1,024 decoded bytes. The resulting text goes through the existing EPUB XML
preparation and DTD-disabled parsing rules.

Previously readable UTF-8 entries keep their original text, package revision
and source anchors. BOM-marked UTF-32LE/BE XML is also accepted as a
compatibility extension: incomplete units, invalid Unicode scalars, NULs and
conflicting declarations are rejected under the same entry bounds. BOM-less
UTF-16 without the declaration signature and BOM-less UTF-32 remain unsupported.
These additions have not yet been behaviorally
compared with KOReader or verified with real books, in the simulator or on
Kindle. [Pinned CREngine encoding detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L6108-L6122)

For compatibility with nonconforming EPUBs, an XML entry that fails UTF-8
decoding without a UTF-8 BOM can use an explicit legacy encoding in a valid
initial XML declaration of at most 1,024 bytes. Reader supports GBK, GB18030,
Big5, Shift_JIS, EUC-JP, EUC-KR, Windows-1250–1258, Windows-874, KOI8-R,
KOI8-U, IBM866 and Macintosh Roman/Cyrillic through strict `encoding_rs`
decoders. Labels follow WHATWG mappings, which can differ from historical
editions of those encodings; unknown labels, invalid sequences, NULs and
decoded entries over 512 KiB are rejected. Valid UTF-8 and UTF-16 paths keep
their existing source text and anchors. This candidate has not been compared
with KOReader or tested with real books, in the simulator or on Kindle.
[Pinned CREngine XML charset detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L2931-L2959)
