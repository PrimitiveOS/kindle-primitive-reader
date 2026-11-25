# OpenDocument Text reading (implementation pending verification)

The pinned KOReader `frontend/document/credocument.lua` registers `.odt` with
CREngine. Reader now routes ODT files through a bounded offline package adapter
into the existing reflowable book pipeline, including library metadata, reading
position, search, selection and annotations supplied by that pipeline.

The initial adapter reads `content.xml` and optional Dublin Core `meta.xml`,
headings with contents entries, paragraphs, basic unordered and numbered lists,
table cell order, bounded repeated cells and rows, spans, explicit whitespace
and line breaks, inherited bold/italic text styles, local bookmark links, and
local PNG/JPEG images. It validates
archive paths, rejects encrypted entries and XML document types, and places
limits on package size, XML nodes and depth, converted output, and images.
Generated XHTML remains limited to 4 MiB. Documents with more than 512 KiB of
extracted text are partitioned into bounded sections; smaller text remains a
single byte-identical chapter, including markup-heavy documents. The source
file is only read.

This is a **partial implementation, not KOReader rendering parity**: most named
and inherited style properties, full list numbering styles, precise table
layout, external links, drawing geometry, embedded objects, tracked changes, other
image formats and encrypted ODF packages need further work. No behavioral,
compatibility, screenshot, performance or Kindle tests have been run for this
change. Preserve this distinction until tested against real LibreOffice and
other ODT documents and compared with KOReader.


ODT `text:s` and `text:tab` elements now retain actual ordinary spaces and
horizontal tabs in the source text rather than substituting no-break spaces.
Those explicit spans use Parley's break-spaces layout; selection and search
use the preserved source characters. Tab-stop geometry is not yet equivalent
to a full ODF layout engine. The preservation mode is only enabled for ODT;
ordinary EPUB and HTML whitespace extraction keeps its existing behavior.
Authored source, numeric-reference and styled-flow checks have not yet run.


## Large documents and existing reading data

The candidate partitions larger text at bounded XHTML boundaries, reopens
ancestor markup and retains necessary whitespace when a paragraph continues.
It verifies the full normalized text and referenced image ordering against
the original before allowing the partitioned book to open. Each generated
section is checked against the existing 512 KiB text limit. Original heading
contents and local bookmarks are redirected to the first matching destination;
percent-encoded link fragments retain their intended destination. A quote that
crosses an artificial split inside a paragraph does not acquire a newline.

The book identity remains derived from the original generated XHTML. A
source-verified map translates old unsplit text and image anchors to their
new section and byte offset. Reader prepares a complete cloned state and only
commits after a candidate reading view opens and saved highlights can be
resolved. Page-only positions, incompatible revisions, ambiguous mappings or
failed checks stop migration and leave the existing saved data intact. The
earlier experimental converter that substituted no-break spaces has a
different generated book revision; its saved anchors are not currently
translated and are rejected rather than guessed. Bookmarks, custom contents,
hidden-flow markers and highlight endpoints are included in the transaction.

The splitter, state migration, cross-part quote handling and authored checks
are implementation candidates; none have yet been built, compared with
KOReader, run in the simulator or verified on Kindle.
