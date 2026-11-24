# Semi-auto PDF crop

In a paged PDF, open **Scan → Semi-auto crop**. Reader shows the full
page. Tap near a corner or side handle to adjust the area Reader should
search. **Apply** trims the margins inside that area. **Cancel** returns
to the same page, zoom, pan and footer.

The seed uses upright page points. Applying a seed saves it for the
current physical page and its odd or even page group. Reader looks for an
exact page seed first, then the matching parity, then the most recent
preceding exact seed. It finds the content again on each subsequent page.
When detected content is under 10% of the original page in both axes, it
uses the seed area. Link and highlight coordinates stay with the original
PDF. Cropped-out targets are outside the touchable area.

The detector uses a bounded grayscale PDF raster and independently
authored projection bounds with the stock default fitted page margin.
The editor supports tap adjustment for the eight handles. Page, width,
height and tile zoom settings persist when a crop is applied. To use
continuous reading, remove the active crop with **Scan → Crop → Reset**.

On the authored four-page PDF, all 16 source edges agree within 0.001
point with the pinned KOReader MuPDF and k2pdfopt runtime at 1,264 by
1,680 and 300 DPI. The fixture covers opposite margin marks, different
content bounds on pages using the same seed, and the small-area
fallback. Reader's grayscale detector can produce different bounds on
other scans from k2pdfopt's cleanup and trimming.

To reproduce, set `READER_SEMI_AUTO_ORACLE_FIXTURE` to a scratch PDF path
and run `cargo test --lib semi_auto_crop -- --nocapture`. Run
`verify-stock-pdf-crop.lua` with that PDF from the pinned KOReader runtime
directory using its `luajit`, an empty `KO_HOME`, and emulator dimensions
`EMULATE_READER_W=1264`, `EMULATE_READER_H=1680`,
`EMULATE_READER_DPI=300`. Set `KO_DYLD_PREFIX` to the native dependency
staging directory for the macOS build. The Rust test records and checks
the native provider result. [Oracle harness](../scripts/verify-stock-pdf-crop.lua)

[Simulator capture](screenshots/semi-auto-pdf-crop.png) · [KOReader crop mode and controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L98-L120) · [KOReader source-area resolution](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/document.lua#L272-L293) · [KOReader detection and fallback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L244-L273)
