# Adobe-obfuscated EPUB publisher fonts

Reader can decode a manifest-declared EPUB font marked with Adobe's legacy
font-obfuscation method using a canonical UUID identifier from the package.
It transforms only the first 1,024 bytes of the selected font as it is loaded.
A book can contain Adobe- and IDPF-obfuscated fonts at the same time; each
font uses its declared method. Raw EPUB resources and the original file are
unchanged.

The reader checks the encryption metadata and font manifest, limits metadata
and face counts, and leaves unknown methods, invalid identifiers, extra
transforms and duplicate declarations untouched. It accepts a hyphenated UUID
with an optional lowercase `urn:uuid:` prefix; other identifier forms fall back
to the available fonts. This does not add support for encrypted book content.

KOReader's pinned CREngine implements the Adobe 16-byte XOR key and 1,024-byte
prefix alongside its IDPF method, selecting the first valid identifier in
package order. Reader accepts a narrower canonical UUID form. Source tests for
key parsing, prefix bounds, mixed-method books, unchanged non-font resources and
font-rendered pages have been added but not run. The KOReader comparison,
simulator and Kindle validation remain pending. [CREngine Adobe handling](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L653-L681) · [Reader decoder](../src/epub_font.rs)
