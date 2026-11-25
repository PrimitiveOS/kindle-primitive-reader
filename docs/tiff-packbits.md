# Compressed TIFF images

Reader accepts TIFF 6.0 PackBits compression for the grayscale and bilevel
strip layouts already supported by its image reader. The existing size limits,
strip ordering, photometric interpretation and bit order apply. Each row must
decompress to its declared width; truncated input, packets that cross a row,
overlapping strips and unexpected trailing data are rejected. This applies to
both standalone TIFF images and TIFF pages opened by the comic image reader.

Reader also implements TIFF LZW compression and the Adobe and legacy Deflate
compression identifiers for the same grayscale and bilevel strip layouts.
Each LZW strip starts with a fresh dictionary and uses TIFF's early code-width
changes. Deflate output must fit the strip's declared dimensions. Both support
horizontal differencing for 8-bit grayscale samples, reversing it independently
for each row. Compressed streams with reverse fill order, other bit depths,
and other predictor modes retain the existing unsupported-format fallback.

KOReader registers TIFF with its MuPDF document provider. Its pinned MuPDF
decoder selects the run-length filter for compression 32773. Reader implements
the TIFF 6.0 row-oriented PackBits format directly, without copying MuPDF code.
The latter also supports other compression and color modes, so this is a subset
of KOReader's TIFF support, not complete TIFF parity. [KOReader TIFF provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L410-L415) · [Pinned MuPDF TIFF decoder](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/load-tiff.c#L360-L369) · [PackBits filter](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/load-tiff.c#L408-L418) · [TIFF 6.0 specification](https://pdfa.org/wp-content/uploads/2020/07/TIFF6.pdf)

These additions have not yet been compared against KOReader or exercised on
the simulator or Kindle. Color, multi-page and tiled TIFFs, non-default
orientation and CCITT remain outside this decoder's support. [TIFF 6.0 LZW
and predictor specification](https://pdfa.org/wp-content/uploads/2020/07/TIFF6.pdf)
