# EPUB cover-page inference

When no EPUB 2 or 3 cover is declared, Reader can infer a library cover from
an image-only first HTML/XHTML spine item, provided the spine has at least two
items. This follows a narrower version of the pinned CREngine cover-page
heuristic: the first item must be unique and at most 5,000 bytes, its body
must contain exactly one local image and no non-whitespace text or script/style
elements, and the image must resolve uniquely to a manifest JPEG, PNG or GIF.
As a later fallback, an SVG wrapper containing one local raster `<image>` and
optional groups is also recognized. It rejects extra drawings, nested foreign
elements, ambiguous `href`/`xlink:href`, scripts and unsupported SVG attributes.
The preview uses the raster image itself, so wrapper transforms or cropping
may not be reproduced.
Explicit or ambiguous cover declarations are left untouched; stock's
`alt="cover"` text exception is not inferred. The book, reading
source and anchors are unchanged. Rendering, memory and parity with real books,
KOReader, simulator and Kindle remain unverified. [CREngine cover heuristic](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L349-L465)
