# Highlight drawing styles

Open the upper reading bar and choose **Settings › Highlight style**. Tap a style to use it for future marks in the open book. **Set reader default** also selects it for books that follow the Reader default. **Use reader default** clears the open book’s override. Each existing mark retains its own style. [Style picker](screenshots/highlight-drawing-picker.png)

Tap a saved passage to change its appearance. If more than one passage covers the word, choose one first. Choose **Actions** when a note is open, then **Style**. Choosing a style returns to the passage at the same location. [Four styles on an annotated page](screenshots/highlight-drawing-page.png)

| Style | Appearance on a grayscale page |
| --- | --- |
| Lighten | Shades the passage. Black print stays black, white paper becomes gray. |
| Underline | Places a gray rule along the lower edge of each line of selected words. |
| Strikethrough | Draws a dark rule through the middle of each selected line. |
| Invert | Reverses the passage’s light and dark tones, including the text. |

Reader records the style with the original EPUB text anchors or fixed-page coordinates. Editing a style keeps its note, source selection and page position. Older saved highlights open as Lighten. KOReader annotation imports recognize its four drawer identifiers. PDF copies use the standard Highlight, Underline and StrikeOut annotations. An inverted mark uses a Highlight with a Difference appearance and a Reader style identifier. PDF tools that regenerate appearances can render this as a conventional highlight. Writing an annotated PDF uses Reader’s existing separate-copy workflow. [Persistent annotation data](../src/library.rs) · [KOReader sidecar import](../src/koreader_sidecar/annotations.rs) · [PDF import and export](../src/pdf_annotations.rs)

## Comparison with KOReader

The comparison uses KOReader’s four saved drawing styles, its book and reader defaults, and its per-annotation edit action. On the 1264 × 1680 Oasis reference grid, Underline is four pixels thick. Strikethrough is three pixels thick. The fixed-page strike begins two pixels lower than the reflow strike for the same box. Lighten at the stock 20% opacity maps six evenly spaced input tones to 0, 41, 82, 122, 163 and 204. Invert maps them to 255, 204, 153, 102, 51 and 0. Configurable highlight colors, percentage height, shading intensity and note margin symbols remain separate controls. [KOReader preferences](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhighlight.lua#L396-L422) · [Per-mark editing](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhighlight.lua#L2340-L2366) · [Reference drawing](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L659-L710)

The reference script loads the installed KOReader drawing function and compares six tones across twenty rows in reflow and fixed modes. Reader tests exercise the reading-bar picker, saved-mark popup, dark and light pages, rotated and zoomed PDF coordinates, old saved data, overlapping drawings and PDF round trips. The screenshots above use the project’s authored Field Notes book. [Independent KOReader check](../scripts/koreader-highlight-drawers.lua) · [Reader pixel rules](../src/highlight_style.rs) · [Reading controls and persistence tests](../src/app/highlight_style_tests.rs)

A physical Kindle Oasis 3 ran six authored functional checks and both independent original-document cases with isolated Reader profiles. The EPUB contributed 53,560 pixel comparisons, and the high-zoom PDF contributed 495,616, across the four styles in light and dark mode. The PDF and EPUB retained their original checksums, anchors and notes. Applying Invert to two overlapping marks restored every pixel of their shared passage. The authored picker and reading-page captures matched their desktop frames across all 2,123,520 pixels per image. The host suite passed 538 unit, 14 integration and 4 system tests.
