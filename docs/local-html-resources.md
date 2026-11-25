# Local resources in HTML and Markdown documents

When opening a standalone XML-compatible HTML or XHTML document, Reader
snapshots referenced local images, linked CSS and supported CSS dependencies
from the document's own directory. Supported image paths are PNG, JPEG, GIF,
SVG, BMP, portable anymap and supported simple still WebP; an inline SVG can
reference supported local images.
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
Well-formed XML-compatible HTML retains its existing conversion and local
resources. If that path fails, a document starting with an `<html>` element,
a plain HTML doctype, or a supported fragment container such as `<p>`, `<div>`,
`<ul>`, `<ol>`, `<h1>`–`<h6>`, `<section>`, `<article>` or `<body>` can use a
bounded text-only recovery path. This fallback
retains supported text and fragment links, but drops images, publisher CSS,
external links and head metadata, and does not cover arbitrary malformed HTML.
External doctypes and entity declarations are rejected. Previously readable
documents retain their existing conversion and anchors.

The text-only recovery can also recognize an optional XML declaration followed
by complete HTML comments before the root, fragment or plain HTML doctype. This prolog is
limited to 2,048 UTF-8 bytes after leading whitespace, and malformed declarations,
invalid or unterminated comments, external doctypes and entity declarations are
rejected. Existing successful conversions keep their original source identity.
This is narrower than KOReader's HTML detection and has not yet been behaviorally
compared or verified on Kindle. [Pinned KOReader HTML detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L6097-L6198)

The loose-document decoder additionally accepts strictly valid
BOM-marked UTF-32LE and UTF-32BE; BOM-less UTF-32 is not detected. KOReader's
pinned CREngine resolves non-container HTML resources against
the document directory; Reader applies the narrower boundaries above.
[KOReader resource resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L995-L1011)

HTML, XML and FB2 can also use strict BOM-less UTF-16LE or UTF-16BE when the
file begins with an XML declaration in that byte order. The declaration must be
well formed, end within 1,024 decoded bytes and either omit its encoding or
declare matching `utf-16`, `utf-16le` or `utf-16be`. Incomplete units, invalid
surrogates, NULs and conflicting declarations are rejected. Plain text and
UTF-16 documents without this opening signature are not detected by this
fallback. Formerly readable documents retain their conversion and anchors.
Real-book, simulator and Kindle verification remain open. [KOReader UTF-16 signature detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L6109-L6121)

For standalone HTML that would otherwise fail decoding, a bounded scan of the
first 1,024 bytes also recognizes supported legacy `http-equiv="Content-Type"`
charset declarations. Existing successful input, BOM and XML-declaration
precedence are unchanged. The chosen encoding must decode strictly; unknown,
truncated or conflicting declarations and broader HTML encoding detection may
still fail. This compatibility path remains unverified on real books, in the
simulator and on Kindle. [KOReader charset detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp#L5877-L5914)

The CSS dependency and import changes passed a Mac build-only check. Containment
and symlink races, image and stylesheet appearance, text-only recovery,
missing-file fallback and saved position preservation still need behavioral,
simulator and device verification before release.

For HTML, HTML ZIP, Markdown and Markdown ZIP, supported simple VP8/VP8L WebP
illustrations are converted to in-memory grayscale PNG under the original
resource key. Both encoded input and converted output must fit the existing
resource budgets; unsupported or oversized images are skipped. The original
files, XHTML and reading anchors stay unchanged. VP8X, animated WebP and the
lossless decoder's [memory and verification limits](webp-lossless-allocation-audit.md)
remain open; these rendering paths have not been behaviorally tested.
