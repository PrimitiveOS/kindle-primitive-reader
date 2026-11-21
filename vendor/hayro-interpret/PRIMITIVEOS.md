# PrimitiveOS Reader’s Hayro interpreter

This copy comes from the published `hayro-interpret` 0.9.0 crate. Its upstream revision is `3f3825c3e50bb41ed6ec6c3f4b6007978bd13def`. The source remains available under Apache 2.0 or MIT. The bundled reference profiles keep the terms in `assets`. [Upstream source](https://github.com/LaurenzV/hayro/tree/3f3825c3e50bb41ed6ec6c3f4b6007978bd13def/hayro-interpret) · [Apache license](LICENSE-APACHE) · [MIT license](LICENSE-MIT)

Hayro’s public image callback receives pixels after color conversion. This patch sits directly at the original conversion method.

The optional `cmyk-cache` feature adds a bounded color cache in `src/color/device_cmyk` before the existing DeviceCMYK ICC transform. Reader enables it on 32-bit ARM. Its two tables and three chunk buffers hold 2,449,408 bytes during each large image conversion. The cache compares all four input color components and stores the original transform’s RGB result. It processes large images in chunks of 8,192 pixels, releases the cache at the end of each image, and falls back to the ordinary conversion if memory is unavailable or the first 65,536 pixels show little repetition. It preserves the source image dimensions and precision. An authored print chart in the module tests checks every output component against the original reference transform.

The rest of the source, including the standard printer-color ICC profile, is unchanged from the published crate. The generator builds the print-chart fixture from eight original swatches.

Run `cargo test --manifest-path vendor/hayro-interpret/Cargo.toml --features cmyk-cache --lib repeated_colors::tests` from the Reader checkout to compare all palette outputs to the original transform. To run the Reader’s own PDF chart test with the cache forced on the desktop, use `cargo test --features hayro-interpret/cmyk-cache --lib printer_color_chart`.
