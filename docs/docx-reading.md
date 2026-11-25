# Reading Word documents

The Word reader keeps local bookmark destinations and internal hyperlinks in
paragraphs, including links commonly used by a document's table of contents.
It also loads embedded PNG and JPEG images referenced from DrawingML, including
illustrations inside tables. Image-only documents can open. Bookmarks use
encoded identifiers so their names cannot alter the generated XHTML; duplicate
destinations retain the first occurrence.

Reader loads only local image relationships, within limits of 128 images,
4 MiB per image and 16 MiB total. Existing restrictions on scripts, embedded
objects and external package resources remain. Unknown image formats and
legacy VML drawings are not rendered. Positioning, cropping and other drawing
effects are not yet reproduced; images appear in the reflow text order.
External hyperlinks remain plain text. Complex fields, notes and broader
WordprocessingML features remain incomplete.

KOReader registers DOCX with both CREngine and MuPDF. Its pinned CREngine
DOCX importer emits local bookmark targets, resolves hyperlink anchors and
uses embedded drawing relationships for images. Reader independently converts
the supported subset to its existing reflowable document pipeline. [Provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1588-L1591) · [Bookmark targets](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/docxfmt.cpp#L1273-L1281) · [Hyperlinks and drawings](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/docxfmt.cpp#L1870-L1930)

This implementation has not yet been compared with KOReader or exercised in
the simulator or on Kindle. Full DOCX parity is not claimed.
