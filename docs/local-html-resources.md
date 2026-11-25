# Local resources in HTML and Markdown documents

When opening a standalone XML-compatible HTML or XHTML document, Reader
snapshots directly referenced local images and linked CSS from the document's
own directory. Supported image paths are PNG, JPEG, GIF, SVG, BMP and portable
anymap; an inline SVG can reference the same local images. These resources use
the existing reading and image-rendering paths. The document's XHTML, source
revision and saved reading positions are unchanged.

Standalone `.md` and `.markdown` documents use the same bounded resource
snapshot for images produced by Markdown image syntax. Their generated XHTML,
publication revision and saved anchors stay unchanged. Loading an image can
change pagination, so a saved page number may change while its source anchor
remains the same. `.md.zip` can also snapshot directly referenced images from
inside the archive beneath the Markdown member's directory. Reader does not
read files beside the archive or extract its contents to disk. Parent-directory
references, nested dependencies of SVGs and other archive resources remain
unsupported.

On Mac and Kindle, each resource path is opened relative to an already-open
document directory. Every component is opened without following symbolic
links, and only regular files are read. Absolute, remote, invalid and escaping
paths, missing files and unsafe resources are skipped; the document text remains
available. Reader snapshots at most 64 distinct resources, 2 MiB per image,
256 KiB per stylesheet and 8 MiB total. Other platforms currently leave these
local resources unavailable. Files are read only and are never extracted or
written.

This covers direct `img` and SVG `image` references and linked stylesheets.
It does not yet discover resources referenced only from CSS `url()` or
`@import`, inline SVG data URLs, fonts, or links to separate HTML chapters.
As in the previous adapter, standalone HTML must be well-formed XML-compatible
markup. KOReader's pinned CREngine resolves non-container HTML resources against
the document directory; Reader applies the narrower boundaries above.
[KOReader resource resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L995-L1011)

This implementation has not been built or tested. Containment and symlink
races, image and stylesheet appearance, missing-file fallback and saved
position preservation need simulator and device verification before release.
The Markdown extension has likewise not yet been built or tested.
