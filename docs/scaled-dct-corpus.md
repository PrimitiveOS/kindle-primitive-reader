# Print JPEG corpus comparison

Reader tested a minimum of three decoded samples per display pixel in both axes. Twelve pinned upstream CMYK or YCCK fixtures produced 36 image views. Ten views used reduced decoding. All 26 views that used the full decoder produced byte-identical full viewport PNGs. The first experiment’s Kindle timings and original chart captures remain available. [Kindle comparison](scaled-dct-experiment.md)

## Coverage

| Project | Source examples | Reduced-decoding cases |
| --- | --- | --- |
| ImageSharp v1.0.4 | Direct CMYK, YCCK, four separate baseline component scans, unusual subsampling | Direct CMYK and interleaved YCCK |
| TwelveMonkeys | 4,256 × 2,832 and 2,707 × 3,804 YCCK photographs with large multisegment JPEG profiles | Both photographs |
| Skia | 642 × 516 Adobe CMYK decoder fixture | Direct CMYK |
| PDF.js | Two original PDF image streams, including a 1,800 × 2,330 image with an explicit PDF decode array | Full decoder for both |
| Apache PDFBox | Small CMYK input generated for PDFBOX-6235 | Full decoder |
| Imageflow and zune-image, through Imazen’s codec corpus | Progressive YCCK logo and small CMYK JPEG | Full decoder for both |

The manifest records each file’s revision, retrieval URL, SHA-256 and upstream license reference. Some projects document the repository’s software license without a separate image grant. The TwelveMonkeys bright sample also has a creator copyright embedded in EXIF. The comparison fetches those files into the requested local directory and publishes numerical results. [Source manifest](../tests/fixtures/dct/public-sources.json)

The wrappers embed the original JPEG bytes without transcoding. Inputs extracted from PDFs retain their supported image dictionary parameters, including explicit decode arrays. Each wrapper prints the image on a 1264 × 1606 page. The fit view has eight source pixels per displayed pixel. The crop view has 6.4 source pixels per displayed pixel and clips the left and bottom edges of the image. The detail view has three source pixels per displayed pixel, which invokes full decoding at the configured guard.

## Image quality

The script assigns the two decoder settings to A and B using the source hash. It computes the difference for the interior of the painted image, excluding the one-pixel border, before resolving the labels. It compares the same region against PyMuPDF 2.1 using MuPDF 1.28.6 in grayscale. The fixed limits were a mean absolute difference of 0.75 on a 0–255 scale, 99th percentile difference of eight, block SSIM of at least 0.995, at most 0.01% of pixels differing by more than one of sixteen shades and at most 0.003 decrease in SSIM against MuPDF.

| Worst measurement among the five eligible images | Fit | Cropped |
| --- | ---: | ---: |
| Mean absolute difference | 0.6751 | 0.5574 |
| 99th percentile difference | 5 | 6 |
| Lowest 8 × 8 block SSIM against full decoding | 0.99800 | 0.99797 |
| Pixels differing by more than one quantized shade | 0% | 0% |
| Largest decrease in SSIM against MuPDF | 0.00034 | 0.00065 |

One input exposed a material error in the earlier experiment. ImageSharp’s separate-component sequential JPEG lost low-contrast detail, with fit-page SSIM against MuPDF dropping from 0.9412 to 0.1610. Reader now requires a complete interleaved four-component baseline scan and returns to full decoding for that input. Four other baseline images carried 559–1,787 KiB of header metadata. The experimental decoder can now skip up to 256 header segments, with a 16 MiB cap on the entire compressed image. [All measured views](../tests/fixtures/dct/desktop-results.json)

The candidate passed 411 host library and integration tests. Two additional opt-in tests exercised highlight extension on independently checksummed original EPUB and PDF files in temporary reading profiles. The three JPEG header regression tests cover the Adobe marker, baseline precision and components, bounded scanning across large profiles, malformed lengths and the noninterleaved fallback.

## Kindle Oasis 3

The device rendered the same 1264 × 1606 viewport in separate processes. Each cell gives the page raster time in seconds and peak process memory in KiB. The original publication has 1,041 pages. At fit page the first pair ran full decoding first and the second pair ran reduced decoding first.

| Page and zoom | Full, seconds / KiB | Three-sample guard, seconds / KiB |
| --- | ---: | ---: |
| Original cover, fit, first pair | 29.205 / 185,700 | 13.856 / 153,200 |
| Original cover, fit, reverse pair | 28.597 / 185,708 | 14.694 / 153,204 |
| Original cover, 150% | 28.641 / 187,192 | 31.362 / 187,192 |
| Skia direct CMYK crop | 0.412 / 14,920 | 0.397 / 14,540 |
| ImageSharp noninterleaved YCCK | 0.512 / 15,608 | 0.496 / 15,616 |
| Authored fine print and lines, 150% | 6.244 / 78,636 | 4.255 / 78,632 |

The three-sample guard reduced original fit-page raster time by 49–53% across the two orderings. Peak memory fell by 32,500–32,504 KiB. Installed KOReader’s MuPDF rendered the original in 5.785 seconds at fit (38,564 KiB) and 8.490 seconds at 150% (38,188 KiB).

The two original photo regions had SSIM values of 0.99850 and 0.99738 against full decoding. Both met the desktop criteria against installed MuPDF. The Skia crop reached 0.99964 SSIM against full decoding. At 150% the entire original cover and authored chart matched full decoding byte for byte. The noninterleaved control also retained its exact full frame. These three full-decoding controls show how much a single timing can vary on the device. [All device measurements](../tests/fixtures/dct/oasis-results.json)

This branch exposes reduced decoding through the experimental build feature and explicit measurement switch. The released Reader uses the full decoder.

## Reproduction

The comparison uses Python 3.12, NumPy 2.5.5, Pillow 13.0.0 and PyMuPDF 2.1. It needs network access for the initial fixture downloads. Subsequent runs verify and reuse the cached sources. The results file includes the decoder executable hash and, for each view, the number of reduced images and both reference SSIM scores.

```sh
cargo build --release --locked --features experimental-scaled-dct --example measure_scaled_dct
uv run --with numpy==2.5.5 --with Pillow==13.0.0 --with PyMuPDF==2.1 \
  python scripts/compare-pdf-dct-corpus.py target/release/examples/measure_scaled_dct build/dct-corpus
```
