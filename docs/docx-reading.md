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

Numbered and bulleted Word paragraphs currently lose their list markers.
KOReader reads the numbering part and renders nested list structures. Reader
cannot simply add those structures to its generated XHTML: its existing saved
positions and highlights use that XHTML's revision and source-byte keys. A
future implementation needs an independently verified out-of-band marker
projection or a source-verified anchor migration. The current MOBI link
projection requires matching element structure and cannot directly project
Word list wrappers.

Macro-enabled Word documents (`.docm`) use the same bounded reflow subset.
The DOCM importer reads only the main document, its local styles, metadata,
and supported embedded images. It ignores external targets and unrelated
package relationships, skips known VBA, control and embedded-object paths,
and rejects loaded content that aliases a known active relationship target.
Content type declarations can be misleading, so this is not a guarantee about
the provenance of all bytes in a malicious package. The importer never executes
macros or Word fields. Inline scripts, alternate content and
embedded objects are currently rejected rather than displayed. A DOCM file
may therefore lose unsupported content or fail to open; this is not complete
Word fidelity. DOCX retains its existing rejection of active package content.
Direct DOCM handoff from the separate Files application is implemented in its
working tree but has not yet been built, published or verified on Kindle.

KOReader registers DOCX with both CREngine and MuPDF and DOCM with CREngine.
Its pinned CREngine DOCX importer emits local bookmark targets, resolves hyperlink anchors and
uses embedded drawing relationships for images. Reader independently converts
the supported subset to its existing reflowable document pipeline. [Provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1588-L1591) · [Bookmark targets](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/docxfmt.cpp#L1273-L1281) · [Hyperlinks and drawings](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/docxfmt.cpp#L1870-L1930)

This implementation has not yet been compared with KOReader or exercised in
the simulator or on Kindle. DOCM behavior has not yet been tested. Full Word
format parity is not claimed.
