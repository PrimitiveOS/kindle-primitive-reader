# Additional reading file extensions

Reader recognizes `.epub3` as an EPUB, including extracted publication folders,
and uses the existing EPUB parser, content limits, positions, whole-book page
index and embedded-font handling. `.log` uses the plain-text reader. `.log.zip`
and `.md.zip` use the bounded single-document ZIP importer, requiring exactly
one `.log` or `.md` member respectively. `.html.zip` and `.htm.zip` use the
HTMLZ importer, which requires one top-level HTML file and supports its bounded
local CSS and image resources. These ZIP aliases do not implement KOReader's
general mixed-archive member selection. [Compressed text](text-zip-reading.md)

An `.xml` file with an XHTML `html` root uses the standalone XHTML reader; one
with a FictionBook root uses the FB2 adapter, including its namespace and
structure checks. Other XML is shown as literal source text so that markup and
values remain visible; Reader does not claim general XML stylesheet or document
layout support. XML and other content still have the existing input and layout
size limits. The file suffix chooses a provider, and the provider validates the
actual content before reading it.

KOReader's pinned CREngine registration lists these suffixes, but its archive
selection and general XML rendering support are broader than these bounded
adapters. The new aliases and authored malformed/ambiguous ZIP checks have not
been built or tested in the simulator or on Kindle. [Pinned registrations](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1594-L1624)
