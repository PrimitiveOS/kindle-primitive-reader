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
Until chapter splitting is implemented, converted XHTML is limited to 512 KiB
so the single chapter fits the native text-layout limit.
The source file is only read.

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
