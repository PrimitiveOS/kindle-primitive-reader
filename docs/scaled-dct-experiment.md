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

## Reproduction

The chart generator takes one output path and requires Pillow. The example accepts a PDF, a one-based page number, a zoom factor relative to fit page and an output PNG path. It prints elapsed page raster time and, on Linux, process peak resident memory. The example viewport is 1264 × 1606 pixels.

```sh
mkdir -p build/scaled-dct
python3 scripts/generate-scaled-dct-chart.py build/scaled-dct/chart.pdf
cargo build --locked --release --features experimental-scaled-dct --example measure_scaled_dct
READER_EXPERIMENT_DCT=0 target/release/examples/measure_scaled_dct build/scaled-dct/chart.pdf 1 1 build/scaled-dct/full.png
READER_EXPERIMENT_DCT=2 target/release/examples/measure_scaled_dct build/scaled-dct/chart.pdf 1 1 build/scaled-dct/reduced.png
```
