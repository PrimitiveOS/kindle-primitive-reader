# Image style tweaks

Open **Settings › Style tweaks › Images** from the reading toolbar. You can
also open **Text › Rendering › Style tweaks › Images** from the bottom
toolbar in a reflowable book. Each preset can follow the default, override
it for this book, or become the default for all books. [Reader settings](screenshots/image-style-reader-menu.png)

- **Full-width images** puts each image on its own line. It requests
  the text column’s width and respects publisher maximum sizes and Reader’s
  viewport bounds. When the publisher specifies a height, widening can change
  the image’s aspect ratio. [Reader page](screenshots/image-style-reader-full-width.png)
- **Vertically center-align images relative to text** aligns inline images
  to the middle of the surrounding type. It can be enabled alongside the
  other options. An explicit publisher alignment still applies.
- **Fit images to text width** retains the existing aspect-fit behavior.
  It is an alternative to **Full-width images**.

The pinned KOReader presets request a block image at full column width and
normal-priority middle alignment for inline images. When a publisher maximum
width prevents the image from filling its line, the block starts on the
leading edge of the text column in both readers.
[Pinned presets](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L705-L727) ·
[Stock original](screenshots/stock-image-style-original.png) ·
[Stock middle alignment](screenshots/stock-image-style-middle.png) ·
[Stock full-width images](screenshots/stock-image-style-full-width.png) ·
[Stock right-to-left placement](screenshots/stock-image-style-full-width-rtl.png)

The stock replay invokes both native callbacks and compares screen output and
source positions. Reader’s Mac tests cover each style, independent selection,
publisher alignment, mixed prose, preserved image links and text anchors, and
per-book and default settings after reopening. The test publication stays
byte-identical. [Stock replay](../scripts/koreader-image-style-check.lua) ·
[Reader layout tests](../src/parley_layout/image_style_tests.rs) ·
[Reader controls test](../src/app/style_tweaks/tests.rs)

Run `cargo test --locked --features kindle --lib image_style`. Set
`READER_SELECTION_VIEWPORT_CAPTURES` to a directory to save the two Reader
screenshots.
