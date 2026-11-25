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

If the normal conversion finds no readable text or supported images, Reader
now retries previously rejected image-only DOCX/DOCM files with local BMP and
first-frame GIF drawings. The same package, relationship, image-size and
aggregate limits apply, and DOCM content types and blocked targets are still
checked. The normal successful conversion remains unchanged, so BMP/GIF
drawings in documents that already opened are not added by this fallback.
Exact KOReader BMP/GIF behavior, real-document rendering, simulation and
Kindle verification remain open.

If both earlier conversions find no readable content, an additional candidate
fallback can load local PNG, JPEG, BMP or first-frame GIF images referenced by
legacy `w:pict` and `v:imagedata` relationship IDs. It uses the same validated
package relationships, blocked-target and resource limits as other Word
images. External targets, embedded objects and VML effects are not rendered.
Previously successful text and image-only conversions retain their XHTML and
anchors. KOReader VML parity and this fallback's real-document, simulator and
Kindle behavior are unverified.

If all earlier conversions find no readable content, a final bounded recovery
can read an OOXML `mc:AlternateContent` element's unique `mc:Fallback` branch
in supported paragraphs, runs and tables. It validates branch order, requires
at least one Choice and exactly one Fallback, and does not render both branches.
The same document-wide script/object checks, local image relationships and
resource limits apply. Alternate content in documents that already opened is
unchanged, and unsupported fallback structures may still fail. This subset is
not a claim of KOReader parity and has not yet been tested with real Word
files, in the simulator or on Kindle.

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
macros or Word fields. Inline scripts and embedded objects are currently
rejected rather than displayed. Alternate content is ordinarily ignored,
with the limited no-readable-content fallback above. A DOCM file
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
