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
the simulator or Kindle. White-is-zero grayscale and palette alpha, CMYK, multi-page and tiled TIFFs and CCITT remain outside this decoder's support. [TIFF 6.0 LZW
and predictor specification](https://pdfa.org/wp-content/uploads/2020/07/TIFF6.pdf)

## Image orientation

Reader applies the TIFF orientation tag after reconstructing the grayscale or
bilevel strips. It supports all eight standard orientation values, including
mirrors and the four transposed orientations that exchange page width and
height. An absent tag means top-left orientation; values outside 1–8 are
rejected. The existing compressed-size, side-length and total-pixel limits
remain in force. Applying an orientation other than 1 briefly needs a second
buffer bounded by the same eight-million-pixel ceiling.

KOReader registers TIFF through its MuPDF provider. The pinned MuPDF TIFF
loader does not appear to interpret the orientation tag; this is a
standard-oriented improvement rather than a claim of identical stock output.
The implementation and authored endian, orientation, malformed-tag and
compressed-bilevel checks have not yet been built or run on the simulator or
Kindle.

## RGB strips

Reader also accepts 8-bit, three-channel contiguous RGB strips and converts
them to the same sRGB grayscale used by its other raster image pages. Stored,
PackBits, TIFF LZW, and Adobe or legacy Deflate strips use the existing bounded
decoders. For LZW and Deflate, horizontal prediction is reversed separately
for each color channel before grayscale conversion. Per-channel bits and
sample-format arrays are checked for uniform 8-bit unsigned samples and their
external ranges cannot overlap each other or image strips. Existing side,
pixel, encoded-byte, orientation and strip bounds apply; RGB strip expansion
and its grayscale output together are bounded to 32 MiB, plus the Deflate
decoder's one-byte overflow sentinel. The compressed input is a separate
bounded buffer.

Contiguous 8-bit RGB and white-is-black grayscale strips with an explicitly
associated or unassociated alpha channel are also implemented, composited over
white. The
combined decoded-strip and grayscale-output allocation is capped at 32 MiB,
plus Deflate’s one-byte overflow sentinel; compressed input remains separately
bounded. These alpha changes are not yet
behaviorally tested. White-is-zero grayscale and palette alpha, CMYK, planar, tiled and
multi-page TIFFs remain unsupported.
KOReader exposes TIFF via MuPDF, whose pinned loader accepts RGB photometric
data and horizontal differencing; this limited subset is not full MuPDF
parity. The authored compression, byte-order, predictor, orientation and
invalid-metadata checks have not yet run on the computer or Kindle.
[KOReader TIFF provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L414-L415) · [Pinned MuPDF loader](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/load-tiff.c)


## Palette-color strips

Reader accepts TIFF photometric palette images with 1, 2, 4, or 8 bits per
index, including rows padded to a byte boundary. It checks the required
16-bit ColorMap count, bounds its storage, rejects overlap with strip data or
other tracked metadata, and converts the mapped RGB colors to the same sRGB
grayscale used for other image pages. Stored, PackBits, LZW and both Deflate
variants use the existing strip decoders. Eight-bit compressed indices support
horizontal prediction; packed indices support either fill order in stored and
PackBits strips. Reverse fill order with LZW or Deflate is still unsupported.
Orientation and existing image limits apply. At most 256 grayscale palette
entries are allocated in addition to the existing bounded strip buffers.

KOReader registers TIFF through MuPDF, whose pinned loader recognizes palette
photometric data and reads the ColorMap. This is a limited local implementation,
not a claim of full TIFF parity. The authored packed-index, endian, orientation,
compression, predictor and invalid-palette checks have not been built or run on
the computer or Kindle. [KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L414-L415) · [Pinned MuPDF loader](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/load-tiff.c)
