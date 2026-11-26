# Compressed RTF and HTMLZ illustrations

Reader recognizes `.rtf.zip` files and opens a single enclosed RTF document
through the same reading pipeline as a plain `.rtf` file. Its compressed file
and expanded text each have the existing 4 MiB RTF limit. A ZIP may contain
at most 64 entries; ambiguous, encrypted, linked, unsupported or oversized
document entries are rejected. Other archive files are not extracted. The
title derived from the filename omits both `.rtf` and `.zip`.

KOReader registers `rtf.zip` with CREngine. CREngine can select from several
eligible document formats in an archive, preferring a default HTML document;
Reader's adapter deliberately accepts exactly one RTF document. Reader's import
retains its current RTF subset; images, field instructions and other hidden
destinations are still not rendered in text documents. [KOReader format registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1617-L1618) · [CREngine archive selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4712-L4793)

As a separate recovery candidate, a structurally valid RTF with no supported
reading text can expose visible direct `\pict` PNG or JPEG groups, including
hex or a single binary payload. It accepts at most 16 images, 1 MiB each and
4 MiB total, and checks image type and bounded dimensions before constructing
an in-memory document. Image contents are decoded later by the existing image
reader. If the direct-picture pass finds no pictures, a second bounded pass can
also recover an otherwise visible `\shppict` wrapper containing exactly one
direct picture; `\nonshppict` alternatives are ignored. Hidden or object
destinations, nested or ambiguous picture groups, mixed or malformed payloads
and other image formats are not supported. A successful direct-picture pass
remains unchanged, including its resource ordering and anchors.
Normal successful text RTF conversion and saved anchors do not change. This
subset has not been behaviorally compared with KOReader or validated in the
simulator or on Kindle. [KOReader picture reader](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/rtfimp.cpp#L305-L360)
The pinned implementation's exact handling of shape wrappers has not been
established; this narrower wrapper fallback is not a parity claim.

Plain and ZIP RTF now decode DOS CP437 (`\pc`, `\ansicpg437`) and CP850
(`\pca`, `\ansicpg850`) text as well as their existing encodings. The mapping
is scoped by RTF groups, and the existing decoded-output limits apply.
Previously readable ASCII documents retain the same generated text and saved
anchors. Unicode's original mapping files and permission notice are included
in the package; [provenance and distribution review](licenses/RTF-OEM.txt)
remain open. This is an RTF compatibility candidate: the pinned CREngine
lists `\ansicpg`, but its `\pc` and `\pca` behavior has not been established.
Real-document, simulation and Kindle comparisons are pending.

RTF text runs can also declare GB18030 (`\ansicpg54936`), KOI8-R (`20866`),
KOI8-U (`21866`) or EUC-JP (`51932`). Reader uses the existing strict
`encoding_rs` decoders and output bounds; malformed encoded runs are rejected.
Their WHATWG mappings, particularly GB18030 and EUC-JP, can differ from older
Windows variants. Previously readable ASCII and supported encodings retain
their conversion and anchors. The pinned KOReader parser lists `ansicpg`, but
its charset setters are incomplete; exact behavior and real-book, simulator
and Kindle comparisons remain open. [Microsoft code-page identifiers](https://learn.microsoft.com/en-us/windows/win32/intl/code-page-identifiers) · [Pinned KOReader charset setters](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/rtfimp.cpp#L642-L655)

HTMLZ archives now retain local GIF and SVG assets as well as the existing
PNG, JPEG and CSS. GIF uses Reader's first-frame decoder and SVG uses its
bounded offline vector renderer. The existing archive resource limits apply.
Scripts and remote resources do not become available through this change.
Other HTMLZ layout and archive restrictions remain in place. [KOReader HTMLZ registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1604)

HTMLZ and the supported HTML ZIP aliases can also retain referenced local BMP,
portable anymap and supported simple WebP images, and TTF, OTF, WOFF and WOFF2
publisher fonts. WebP images are converted to grayscale PNG in memory while
retaining their original resource key; both representations count toward the
per-file and aggregate limits by their larger size.
Reader examines a bounded set of up to 64 references in the XHTML and already
loaded CSS, resolves them inside the validated archive, and reads these extra
members only when each is at most 2 MiB and fits the existing 8 MiB total
resource budget. Missing, ambiguous, invalid or oversized extra members are skipped so
that previously readable text remains available. No contents are extracted to
disk or fetched from the network; source XHTML and saved anchors stay unchanged.
Actual font and image rendering use the existing bounded reader subsets.
This change has not been behaviorally tested on real archives, in simulation or
on Kindle, and full KOReader format parity remains open.

For a selected HTML ZIP entry that fails the XML-compatible path, the same
bounded text-only recovery as standalone HTML can retain basic readable text;
images, publisher CSS, external links and head metadata are dropped in this
fallback. Unsupported declarations remain rejected. Previously readable HTML
entries keep their existing conversion and anchors. Recovery remains unverified.

These changes have not yet been tested in the simulator, compared with
KOReader on matching files, or verified on Kindle.

The image-only recovery candidate also accepts visible `\dibitmap0` pictures
whose payload is exactly a 40-byte BITMAPINFOHEADER followed by uncompressed
1-, 4- or 8-bit indexed rows with a validated RGBQUAD palette, or 24-bit RGB
rows without a palette. Standard four-byte row padding is required; both
bottom-up and top-down rows are accepted. Width must be positive, height must
be nonzero, the optional image-size field must match, palette reserved bytes
must be zero and every visible palette index must be in range. The generated
14-byte BMP file header counts toward the existing 1 MiB per-picture and 4 MiB
aggregate limits. Other DIB headers, 32-bit/alpha, compression, ambiguous or
incomplete payloads remain unsupported. This path only runs when the text importer reports no readable
text; previously successful text and image-only conversions keep their output
and anchors. The DIB and resulting BMP buffers can coexist temporarily, and
BMP decoder internals and actual rendering have not been measured. The pinned
KOReader RTF command table explicitly registers PNG and JPEG picture controls;
its DIB behavior, real-document, simulator and Kindle comparison remain open.
[KOReader picture commands](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/include/rtfcmd.h#L44-L52)
