# JPEG 2000 opacity

Reader opens bounded single-image JP2 files with grayscale or RGB plus a final,
explicitly declared unassociated opacity channel. It composites each pixel
over white for the e-ink page. This also applies to supported JP2 images inside
comic archives. Opaque JPEG 2000 images retain their existing behavior.

The channel definitions must list color channels in order, followed by the
opacity channel associated with the whole image. The existing input, tile,
pixel and component-sample limits apply before decoding; four-channel images
therefore have a lower maximum pixel count. Raw codestreams without explicit
JP2 channel definitions, premultiplied opacity, palette images and multiple
images remain unsupported by this opacity path.

KOReader exposes JPEG 2000 through its MuPDF image provider, whose pinned
decoder counts and handles alpha channels. Reader uses its existing native
JPEG 2000 decoder and checks channel definitions independently. [KOReader registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L398-L403) · [Pinned MuPDF channel handling](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/fitz/load-jpx.c#L488-L520)

This change has not yet been built, tested against sample JP2 files, compared
with KOReader, or exercised on Kindle.
