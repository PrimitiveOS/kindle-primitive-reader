# OCR scan reflow alignment

The pinned KOReader fixed-document reflow panel offers **Auto**, **Left**,
**Center**, **Right** and **Justify**. Auto attempts to preserve alignment from
the original document. [Pinned alignment control](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/koptoptions.lua#L2404-L2455)

PrimitiveOS Reader exposes the same choices in OCR scan reflow's **Layout**
panel for horizontal writing. Its Auto mode estimates the source's dominant
alignment from OCR line geometry and falls back to the selected LTR/RTL writing
direction when there are too few long lines to classify. Left, Center and Right
use physical page alignment. TBRTL uses top alignment and hides the horizontal
alignment choices.
Justify distributes leftover line width across the inter-word gaps except on
the final line of a paragraph or page. These rules operate on word-image crops,
so their raster output is not expected to match KOReader's text renderer.

The choice is saved per fixed document, copied to document defaults and
reading profiles, and omitted from older profile actions that never stored an
alignment value. Applying a new alignment reflows the visible source words
while retaining the original OCR word anchor.

This is an unverified implementation candidate. Deferred comparison should
include left-, center-, right- and justified source pages; short headings;
paragraph-final lines; narrow and wide columns; RTL text; missing line boxes;
profile compatibility; cancel/apply/reopen; and the E Ink refresh behavior.
Capture the same pages from KOReader and PrimitiveOS simulator, then repeat on
the Oasis before updating the parity status to verified.
