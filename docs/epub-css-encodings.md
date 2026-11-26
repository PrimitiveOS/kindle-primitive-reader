# Publisher stylesheet encodings

Linked and imported CSS that is not valid UTF-8 can use a strict BOM-marked
UTF-16LE or UTF-16BE fallback, or an exact leading CSS `@charset "label";`
declaration for supported legacy encodings. The legacy set follows Reader's
existing `encoding_rs` EPUB XML compatibility set: supported Windows code
pages, Japanese, Korean and Chinese encodings, KOI8, IBM866 and Macintosh
variants. Encoding labels follow `encoding_rs` aliases, which may differ from
other decoders' historical variants. BOM-marked UTF-16 must not contain a
conflicting leading charset declaration. Malformed sequences, NULs, unsupported
labels, UTF-32 and nonconforming declarations are rejected rather than guessed.

The fallback accepts at most 96 KiB of encoded and decoded CSS per sheet, and
the existing aggregate, import depth, cycle, font and resource limits apply.
Decoder temporary allocations may exceed the decoded byte count. Existing
valid UTF-8 handling and its inventory limits are unchanged. The chapter and
embedded-font inventory use the same fallback. Publisher files, chapter text
and saved anchors stay unchanged; newly loaded CSS can change pagination and
appearance. KOReader parity, real EPUBs, simulator, device and peak-memory
behavior have not yet been verified.
