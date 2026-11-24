# Optional reading footer separator

KOReader's `Show status bar separator` preference inserts a horizontal line
above the reading footer. Its callback toggles the saved
`bottom_horizontal_separator` option in the pinned
[ReaderFooter source](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L1800-L1806).
The stock default is off.

PrimitiveOS Reader adds **Show footer separator** under **Reading status bar**.
It is off for new and legacy settings. When enabled, Reader paints the line at
the content/footer boundary for reflowable EPUBs and fixed-page documents. It
does not move or change the `Chapter x of y - Page m of n` caption, horizontal
progress track, chapter ticks, or Exit target.

The setting is persisted in Reader's normal controls file. Mac tests verify
default-off behavior, toggle state and persistence, and the exact boundary
pixel for EPUB and PDF. Simulator captures:

- [Footer without separator](screenshots/footer-separator-off.png)
- [Footer with separator](screenshots/footer-separator-on.png)
- [Footer with separator in dark mode](screenshots/footer-separator-dark.png)
- [PDF footer with separator](screenshots/footer-separator-pdf.png)
- [Reading status bar setting](screenshots/footer-separator-settings.png)

Kindle E Ink appearance and foreground acceptance remain open until the new
ARMHF test bundle and UI can run on the device.
