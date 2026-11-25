# Publication language for local documents

Reader's HTML, HTMLZ, FictionBook, DOCX/DOCM and ODT importers retain a
publication language when one is declared in document metadata. That language
is now available as the publication-level fallback for shaping and automatic
hyphenation, using the same bounded language-tag validation as EPUB. Explicit
reader language preferences retain their existing precedence. Empty, invalid
or oversized tags are ignored.

This uses metadata already extracted by each importer and does not change the
generated XHTML, revision, document text or saved source keys. EPUB, MOBI and
FB3 retain their existing language handling. Language selection, rendering and
saved-position behavior for this change have not yet been tested on the
simulator or Kindle.
