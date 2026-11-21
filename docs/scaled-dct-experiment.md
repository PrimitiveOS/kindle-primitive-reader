# Reduced JPEG decoding for print photographs

The cover of an original 1,041-page PDF paints three Adobe YCCK photographs. Its two largest JPEGs contain 15.31 and 16.49 million pixels. One 3,525 × 4,677 image occupies approximately 512 × 680 pixels in Reader’s fit-page view on the Oasis 3. Converting full-resolution images through the print color profile accounted for a substantial part of the previously measured 23.451-second first raster. [Earlier Oasis timings](scan-controls.md#large-print-color-pdfs-on-kindle)

## Image selection

The optional `experimental-scaled-dct` feature enables an experiment with the inverse DCT in `jpeg-decoder` 0.3.2. In a comparison build, `READER_EXPERIMENT_DCT=2` requests at least two decoded samples for each screen pixel along each axis. A value of `3` retains at least three. An unset variable or a value of `0` selects the existing decoder. Reader still applies its own color profile and final resampler. [Decoder scaling API](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/struct.Decoder.html#method.scale) · [MIT notice](licenses/pdf/JPEG-DECODER-MIT.txt)

The alternate path accepts a directly filtered four-component DeviceCMYK JPEG with eight-bit, sequential baseline data, an Adobe CMYK or YCCK marker, and source dimensions matching the PDF. A candidate must contain between 262,144 and 24 million pixels, at most 16 MiB of compressed data and permit a reduction on both axes. Higher zoom, progressive JPEGs, RGB, indexed or profile-based color spaces, masks, transfer functions, explicit PDF decode arrays and filter parameters use the standard path. Failure to identify or decode an eligible image also returns to the standard path.

## Desktop comparisons

The Mac rendered the original illustrated cover, an independently authored large scan containing six sizes of text, registration lines and color swatches, and Reader’s existing small CMYK chart. MuPDF rendered the same pages and viewports in grayscale. Measurements used the 8-bit rendered pixels. The shade comparison rounded each image separately to 16 evenly spaced gray levels.

| Compared with full decoding at fit page | Mean absolute difference (0–255) | 99th percentile | 8 × 8 block SSIM |
| --- | ---: | ---: | ---: |
| Original cover, left photograph | 0.148 | 2 | 0.9984 |
| Original cover, right photograph | 0.427 | 5 | 0.9971 |

At fit page the maximum quantized difference in both photograph regions was one shade. Against MuPDF, the left photograph’s SSIM changed from 0.9603 to 0.9613 and the right photograph’s from 0.9678 to 0.9669. At 150%, the largest measured multiple-shade difference on either photograph was 0.0019% of its pixels. At 300%, the original and authored full PNGs matched the preceding decoder byte for byte.

The OCR comparison counted chart lines for which full decoding reproduced at least 90% of the expected character sequence. The candidate retained 10/10 such lines at fit page, 17/17 at 150% and 20/20 at 300%. The small CMYK chart and a page containing six exceptional inputs produced byte-identical viewport images with the alternate decoder enabled and disabled.

## Kindle Oasis 3 comparisons

The Oasis rendered the same 1264 × 1606 viewport in twelve separate Reader processes and four installed KOReader/MuPDF processes. Each time measures the page raster after parsing the PDF. Peak resident memory was sampled before encoding the output PNG. The source PDF retained its original checksum. Two fit-page passes for each Reader path produced identical full PNGs within that path.

| Document and view | Reader full raster | Reader reduced raster | KOReader grayscale raster | Reader peak KiB, full / reduced |
| --- | ---: | ---: | ---: | ---: |
| Original cover, fit page, reverse-order pair | 26.817 s | 11.223 s | 5.773 s | 185,732 / 153,236 |
| Original cover, 300% | 31.641 s | 28.253 s | 10.027 s | 203,060 / 203,060 |
| Authored print scan, fit page | 4.189 s | 3.585 s | 1.223 s | 78,652 / 42,352 |
| Authored print scan, 150% | 5.387 s | 3.020 s | 1.219 s | 78,668 / 42,376 |
| Small CMYK chart, fit page | 0.458 s | 0.444 s | — | 14,648 / 14,652 |

At fit page three cover images qualified for reduced decoding. In the reverse-order pair the raster took 15.594 seconds less and used 32,496 KiB less peak memory. Installed MuPDF rendered the same viewport 5.450 seconds faster and peaked at 38,564 KiB. A concurrent file transfer overlapped both measurements of the first pair, which took 26.276 seconds with full decoding and 15.338 seconds with reduced decoding. At 300% on the original and at fit page on the small chart, both settings ran the full decoder and produced byte-identical full PNGs. Their single-pass timing differences show run-to-run variability.

| Physical fit-page photograph | Pixels | Mean absolute difference (0–255) | 99th percentile | SSIM vs full decoding | SSIM vs KOReader, full / reduced |
| --- | ---: | ---: | ---: | ---: | ---: |
| Left | 346,112 | 0.144 | 2 | 0.99850 | 0.94737 / 0.94912 |
| Right | 338,676 | 0.408 | 4 | 0.99738 | 0.94613 / 0.94480 |

Two pixels in the right photograph changed by two shades after quantization to sixteen gray levels. All other pixels in both photograph regions changed by at most one shade. In the authored scan, reduction retained all 10 fit-page and 17 enlarged text lines that full decoding reproduced with at least 90% character agreement. At 150%, seventeen pixels across the authored text and registration regions changed by two shades. The desktop three-sample guard returned to full decoding at this zoom for these inputs and rendered the same cover pixels as the two-sample guard at fit page.

These full-size captures show the same independently authored chart at fit page on the Oasis. [Reader full decoding](screenshots/oasis-print-chart-full.png) · [Reader reduced decoding](screenshots/oasis-print-chart-reduced.png) · [Installed KOReader](screenshots/oasis-print-chart-koreader.png)

The standard Reader package continues to use full decoding. The experiment demonstrates a fit-page speed and memory benefit for this print PDF. Assessing the three-sample guard on more independently sourced CMYK and YCCK scans would give a broader picture of fine text and photographic detail before choosing a release default.

## Reproduction

The chart generator takes one output path and requires Pillow. The example accepts a PDF, a one-based page number, a zoom factor relative to fit page and an output PNG path. It prints elapsed page raster time and, on Linux, process peak resident memory. The example viewport is 1264 × 1606 pixels.

```sh
mkdir -p build/scaled-dct
python3 scripts/generate-scaled-dct-chart.py build/scaled-dct/chart.pdf
cargo build --locked --release --features experimental-scaled-dct --example measure_scaled_dct
READER_EXPERIMENT_DCT=0 target/release/examples/measure_scaled_dct build/scaled-dct/chart.pdf 1 1 build/scaled-dct/full.png
READER_EXPERIMENT_DCT=2 target/release/examples/measure_scaled_dct build/scaled-dct/chart.pdf 1 1 build/scaled-dct/reduced.png
```
