# FictionBook documents with an XML extension

An `.xml` document that successfully parses as a FictionBook 2 document is
now treated as FictionBook by the in-memory reading engine. It gains the
FictionBook automatic stylesheet and note-section handling. Detection requires
the exact FictionBook namespace and the same document validation as `.fb2`
opening. Other XML follows its existing HTML or plain-text path.

The path-derived library format and fingerprint remain unchanged, as do the
converted XHTML and its revision. Existing reading positions and highlights
therefore continue to use the same identity and source positions. This
preservation has not yet been verified by running tests.

Kindle reading controls now use the parsed in-memory format for FictionBook
style tweaks, the base stylesheet and its default, and profile choices. When
the two stylesheet defaults differ, a newly opened XML document is parsed once
before the normal opening to select the appropriate default. Saved choices and
profile settings retain their existing precedence. Simulator and Kindle
behavior remain unverified.
