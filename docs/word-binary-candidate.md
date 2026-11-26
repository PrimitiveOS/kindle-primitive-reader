# Binary Word reading candidate

This is a **behaviorally unverified** clean-room Rust candidate for a very
narrow subset of binary Word documents. The local Reader and Files working
trees route `.doc` files to the candidate; Reader has had a Mac build-only
check, while the Files handoff has not yet been built. Neither has been
released. It does not include or invoke
KOReader's Antiword implementation. No claim of Word 97–2003 support or format
parity should be made until routing, meaningful fixture and real-document
validation, simulator checks, device checks and distribution review are done.

The candidate reads an in-memory MS-CFB v3/v4 file under 32 MiB. It bounds FAT,
DIFAT, directory and mini-FAT traversal; rejects cycles, shared sectors,
nested storages, orphan directory entries, duplicate names and unsupported
root streams; and caps accumulated stream bytes. Only `WordDocument`, the
Word table streams, and the two standard summary-information streams are in
its root allowlist. This restriction can reject otherwise valid Word files.
There is no filesystem extraction, external access or macro execution. The
format checks are not a guarantee about the provenance of bytes in a malicious
file. Peak allocations and malformed-file behavior remain unverified.

The Word analysis currently attempts only the original, nonextended Word 97
FIB. It rejects templates, glossary documents, encryption, declared pictures,
auxiliary stories, fields and complex-property CLX records. The main text is
limited to 4 million UTF-16 code units, with at most 8,192 non-overlapping
pieces; compressed characters use the MS-DOC FcCompressed byte-to-Unicode
mapping and uncompressed pieces require valid UTF-16. Unsupported controls are
rejected instead of guessed. The new non-ASCII compressed path has not been
compared with real documents, KOReader or Kindle.

Before text can be converted, a restrictive visibility gate requires complete
character and paragraph FKP coverage, nonoverlapping property pages and text,
no piece properties, default-Normal-only paragraphs with no direct paragraph
properties, and strictly parsed, no-inheritance/no-property Normal and
default-character styles. Direct character runs may contain only bounded,
nonoverlapping MS-DOC bold, italic and strike toggle properties, normal single
underline, and basic superscript/subscript; all other character
properties, including hidden or deleted text flags, remain rejected. The default character style at index 10
must be present in this initial candidate; an empty, inherited or otherwise
unsupported default character style is rejected. These restrictions intentionally reject normal formatted, revised,
hidden, linked, image-bearing or otherwise more complex Word documents. They
are not a substitute for the full Word visibility algorithm; the proof and all
edge cases still need independent review and behavioral verification.

Only after this gate does the candidate construct escaped plain XHTML, capped
at 512 KiB and 32,000 layout elements for a single section. Because the ordinary reflow path collapses horizontal whitespace, this first
candidate rejects tabs, repeated spaces, leading or trailing line spaces and
empty lines rather than silently changing them. Original document bytes are
never modified. The local `.doc` route has a distinct original-file identity; future changes
must preserve or explicitly migrate any anchors when this strict subset expands. This candidate has not been built, packaged or released.

Primary references: [MS-CFB header](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/05060311-bfce-4b12-874d-71fd4ce63aea),
[MS-CFB mini FAT](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/c5d235f7-b73c-4ec5-bf8d-5c08306cd023),
[Word FIB](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/26fb6c06-4e5c-4778-ab4e-edbf26a545bb),
[text retrieval](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/01d5d8c4-cf9c-4ef9-80fd-439e763cfe01),
[compressed characters](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/aa2e55a2-f4f2-4795-bab5-6d9d7a0ed249),
[character formatting](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/be58bf9c-d1d3-40cc-91ee-36452d7939b2),
[default character style](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/7022285b-9621-42e9-ad4d-4e02c115ef18),
[character FKP](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f5f10f04-d4cc-4ebd-86df-0de6d227675c)
and [style inheritance](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/9258b41c-ff0a-4c96-a3a9-610664dabbeb).

The bold/italic candidate maps FKP byte ranges through the original pieces to
UTF-16 character positions, rejects style boundaries that split code units or
surrogate pairs, and renders the supported emphasis as XHTML. Existing
all-default character runs retain their previous XHTML and anchors. Repeated,
unknown or malformed modifiers are rejected, and the existing visibility,
whitespace, size and auxiliary-content restrictions remain. This extension has
not been behaviorally verified or compared with KOReader, real Word files,
the simulator or Kindle. [Microsoft character properties](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/7022285b-9621-42e9-ad4d-4e02c115ef18) · [Toggle semantics](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/bc4ca98b-68ca-49dc-a091-1460e42d781e)

The separate single-underline candidate accepts direct `sprmCKul` values for no
underline or normal single underline, including alongside the existing bold
and italic properties. Other underline variants, duplicate modifiers and all
unrecognized or visibility-affecting properties are rejected. Existing
documents without an underline modifier keep their previous XHTML. This change
has not been behaviorally tested or compared with KOReader, real Word files,
the simulator or Kindle. [Microsoft Kul values](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f6d2ad93-87cf-4e46-a992-a38ccf2ef295)

The direct `sprmCFStrike` and `sprmCIss` candidate admits the documented
strikethrough toggle and normal/superscript/subscript values while retaining the
existing visibility proof and rejecting duplicates or unsupported properties.
It emits semantic `<s>`, `<sup>` and `<sub>` elements; Reader's size and
baseline choices are approximations and have not been compared to Word or
KOReader. Previously accepted documents retain their generated XHTML, and this
addition remains behaviorally unverified. [Microsoft character properties](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/7022285b-9621-42e9-ad4d-4e02c115ef18)
