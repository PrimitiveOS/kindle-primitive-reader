# EPUB chapters with HTML void tags

Some converted EPUB chapters use HTML-style `<br>` or `<img>` tags without
the XML closing slash. If Reader's existing XML-compatible chapter path fails,
it can now try a bounded repair that adds the slash to recognized lowercase
HTML void start tags and accepts the result only if it parses as a complete
XHTML or unnamespaced HTML document with a body. Quoted attributes, comments,
processing instructions and CDATA are handled separately; ambiguous markup
and scripts are rejected. A style block is accepted by this repair only when
its contents contain no literal `<` character. A single exact plain HTML doctype
is permitted only when verified against the original bytes before XML
preparation; other DTDs, entity declarations and ambiguous declarations do not
use this repair.

The applicable XML limit includes inserted characters: 1 MiB for reflow spine
chapters and 512 KiB for navigation documents. Already
parseable chapter content is returned unchanged. Links, image references and
text in a repaired chapter are retained, and navigation uses the same repaired
source. New anchors in such previously unreadable chapters address the
deterministic repaired text; publication files are not modified. This is not
a general HTML5 parser, does not repair arbitrary mismatched elements, and
does not establish behavior for every malformed input. Real-book, simulator,
Kindle and KOReader comparisons remain pending. KOReader's pinned CREngine
has a permissive [HTML/XML parser](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvxml.cpp).

For an otherwise XML-compatible chapter that still fails, Reader can also
escape an unambiguous literal ampersand in text or an attribute, then require
the same bounded complete-document validation. Existing XML references,
comments, CDATA and processing instructions are kept unchanged. Numeric
references, complete unknown names and semicolonless names that could be HTML
entities are not guessed; malformed markup continues to fail. If neither
individual repair succeeds, the same bounded void-tag scan can be combined
with the ampersand repair; the combined result must pass full validation.
After those paths fail, the scanner can also normalize and close up to 8,192
ASCII-case variants such as `<BR>` and `<IMG src="…">`. Other mixed-case
elements and uppercase attributes are not normalized. This fallback also
requires the same complete-document validation.
Source text and anchors for previously readable chapters remain unchanged;
newly readable chapters use the deterministic repaired source. This path is
also untested on real books, simulator and Kindle.
