# Reading print-color photographs in PDFs

Reader chooses the JPEG decode size for the current viewport. A print photograph displayed at a small size can use a reduced inverse DCT while keeping at least three decoded samples for every displayed pixel in both image axes. Zooming, rotating, or cropping recomputes the transformed axes. Small images, pages with fine detail at the current zoom, and vector text follow their existing rendering paths.

## Eligible images

Reader accepts standalone, eight-bit, four-component Adobe CMYK or YCCK baseline JPEGs in PDF DeviceCMYK image objects. The frame and the interleaved scan must identify the same four distinct components. Reader limits the compressed JPEG to 16 MiB, the source area to 262,144–24,000,000 pixels, and the header walk to 256 segments. It chooses a JPEG reduction of 2, 4 or 8 and verifies the actual decoder output size on both axes.

The full PDF decoder handles separate-component scans, progressive and non-baseline JPEGs, other color spaces, masks, chained filters, explicit PDF decode arrays or parameters, transferred colors and ambiguous abbreviated keys. An unsuccessful reduced decode also falls back to the full decoder. Reading positions, page dimensions, selection, links and bookmarks use the original PDF geometry. The build feature `reference-full-dct` supplies a full-decoding comparison executable.

## Comparisons

The desktop comparison covers 36 fit, cropped and detailed views from 12 pinned public fixtures. Ten qualifying views met the predeclared grayscale difference and MuPDF reference limits. All 26 remaining views produced identical whole-frame PNGs with both builds. Eleven additional authored cases exercised ordinary and 90-degree-rotated photographs, detail in one axis, a skewed image, two kinds of masks, explicit decode arrays and parameters, an ICC-based image, RGB and grayscale. All nine full-decoding controls matched byte for byte. [Source revisions and licenses](../tests/fixtures/dct/public-sources.json) · [Desktop results](../tests/fixtures/dct/desktop-results.json)

On the Mac the normal Reader picker opened a byte-verified copy of the original 1,041-page PDF, drew the cover, turned the page and resumed from an isolated profile. Six views across four originals preserved the page count, printed label, page size, text with word and line boxes, links and outline. Only the original cover at fit used reduced decoding. The original at 150%, its second page and all three other interiors matched the full renderer pixel for pixel. One checked interior contains 838 words and two links. Another has 80 outline entries.

We inspected the referenced image streams on all 2,377 pages of the four original PDFs. Seven print JPEG streams on five pages met the initial color and size checks. The other four candidate pages used full decoding both at fit and 150%. All eight full frames matched exactly. In this collection the fit-page cover benefited.

The default production candidate also passed four offscreen Oasis comparisons against a separately compiled full decoder. At fit the original cover used reduced decoding on three image objects and matched the previously measured candidate frame. Its 150% viewport and the two interior pages matched the full decoder byte for byte. Page count and labels, word and line rectangles, link targets, outline and page geometry matched before and after each render. A second isolated test drove the genuine Documents picker, opened the untouched original, turned its page, saved the reading position and resumed. The source files, installed Reader, reading records and Browser session retained their checksums.

The desktop test suite passed 471 checks, plus the five JPEG-header and sampling regressions. Three additional tests used original documents to check EPUB and PDF selection extension and publisher page labels. A lossless desktop simulation shows the independently generated resolution chart at normal fit. [Full-resolution chart in Reader](screenshots/print-image-fidelity.png) · [Chart generator](https://github.com/PrimitiveOS/kindle-primitive-reader/blob/066623d/scripts/generate-scaled-dct-chart.py)

The earlier opt-in build reduced the fit-page raster of a 1,041-page original on Kindle Oasis 3 from 29.205 to 13.856 seconds and from 28.597 to 14.694 seconds in reverse order. Peak process memory decreased by about 32,500 KiB. Installed KOReader’s MuPDF rendered the same fit-page viewport in 5.785 seconds. The two photograph regions scored 0.99850 and 0.99738 SSIM against full decoding. The original and the independently authored fine-print chart retained identical pixels at 150%. [Original Oasis timing and image comparison](https://github.com/PrimitiveOS/kindle-primitive-reader/blob/a9114d917bd2/docs/scaled-dct-corpus.md)

## Reproduce the comparisons

The script fetches the pinned public inputs and validates their SHA-256 hashes. It measures the painted region against a full-decoder build and MuPDF. The authored controls use the included printer profile and generate their own images.

```sh
mkdir -p build/print-check
cargo build --release --locked --example measure_pdf_page
cp target/release/examples/measure_pdf_page build/print-check/guarded
cargo build --release --locked --features reference-full-dct --example measure_pdf_page
cp target/release/examples/measure_pdf_page build/print-check/full
uv run --with numpy==2.5.5 --with Pillow==13.0.0 --with PyMuPDF==2.1 \
  python scripts/compare-pdf-dct-corpus.py build/print-check/guarded build/print-check/full build/print-check/public
uv run --with Pillow==13.0.0 \
  python scripts/check-print-image-fallbacks.py build/print-check/guarded build/print-check/full build/print-check/controls
```
