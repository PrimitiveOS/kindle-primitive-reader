# Local resources in HTML and Markdown documents

When opening a standalone XML-compatible HTML or XHTML document, Reader
snapshots referenced local images, linked CSS and supported CSS dependencies
from the document's own directory. Supported image paths are PNG, JPEG, GIF,
SVG, BMP and portable anymap; an inline SVG can reference the same local images.
Supported font paths are TTF, OTF, WOFF and WOFF2. These resources use
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

This covers direct `img` and SVG `image` references, linked stylesheets, CSS
`url()` references in stylesheets, `<style>` and style attributes, and local
CSS `@import` references. References are resolved relative to the stylesheet
or document containing them. CSS snapshots traverse at most 64 distinct
resources, including imports, with duplicate and cycle handling. The existing
style engine applies unconditional string or `url()` imports in order before
supported rules, within its separate eight-import, four-level, 96 KiB per
sheet and 256 KiB total CSS limits. Font loading also has its existing six-file
and total byte limits. Media-qualified, layered or conditional imports,
arbitrary CSS image rendering, inline SVG data URLs, and links to separate
HTML chapters remain unsupported. Imported CSS uses the existing selector and
property subset; loading an asset does not imply every CSS property renders.
As in the previous adapter, standalone HTML must be well-formed XML-compatible
markup. KOReader's pinned CREngine resolves non-container HTML resources against
the document directory; Reader applies the narrower boundaries above.
[KOReader resource resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L995-L1011)

The CSS dependency and import changes have not been built or tested. Containment and symlink
races, image and stylesheet appearance, missing-file fallback and saved
position preservation need simulator and device verification before release.
The Markdown extension has likewise not yet been built or tested.
