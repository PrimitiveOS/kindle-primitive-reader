# Animated GIF frames

Reader opens a standalone animated GIF as a sequence of still pages. Use the
normal page controls to move between frames. Reader reconstructs each frame
from the beginning, including transparency and the previous frame's GIF
disposal rule. Background disposal clears the affected rectangle to white.
A single-frame GIF continues to open as one still image. GIFs
embedded in books and comics retain the existing first-frame behavior.

The source stays in a bounded in-memory buffer and no file handle is retained
while reading. Reader accepts up to 128 frames and limits the cumulative
frame, canvas initialization and restore-to-previous work to 32 million
pixels. Animated canvases are limited to 4 million pixels; the existing image
dimensions and 16 MiB input limits also apply. An
oversized, malformed or unsupported sequence returns an error rather than
silently becoming a single image. Reader does not play frames on a timer.

KOReader's pinned GIF provider exposes frames as pages and composites prior
frames on demand. Its source clears the full canvas for its background case
and leaves restore-to-previous unimplemented; Reader uses the previous frame's
disposal rectangle and restore-to-previous canvas, with white as the background.
This is a deliberate
behavioral difference, not a claim of pixel parity. [KOReader GIF provider](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/pic.lua#L118-L163)

This implementation has not been built or tested. Frame order, transparency,
disposal, page-button navigation, saved position and resource limits need
simulator and device verification.
