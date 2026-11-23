# Wrapped media-only line source ranges

An authorized original EPUB failed to open three chapters because a full-width
inline picture wrapped to a line with no text run. Parley initialized the line's
text union to `usize::MAX..0`; a picture-only line never updated it. The canonical
projection correctly rejected that invalid range. The earlier loose range use
could also make paint-tail analysis index outside the paragraph.

The bounded vendor fix changes the producer: only when a line has no committed
text run, use an empty range at its actual input inline-box caret. Normal text
ranges retain their existing shaped-cluster bounds. Invalid UTF-8 and out-of-range
text positions still fail; no source characters are fabricated or excluded from
coverage to hide a failure.

The renderer separately retains each picture's exact containing line top.
Rounded framebuffer painting coordinates no longer determine line ownership.
Picture-only pages therefore save the original image ordinal and reopen the same
page, including multiple successive pictures at a paragraph's final text caret.

![An authored projected-caption picture-only page](screenshots/wrapped-media-page.png)

Two authored tests cover six one/two/three-picture layouts, a multibyte caption,
a CSS-removed source prefix, empty caret ranges, coverage, picture ordinals,
serialized page restore, page/scroll captures and strict invalid-text rejection.
All pass on the Mac. The full ordinary suites passed 831 library, 14 package and
4 system tests (849 total; 62 opt-in cases ignored). The three previously failing
original chapters also open with the identical font stack and unchanged source
SHA-256. Whole-original replay checked all 49 sections and painted 2074 pages:
2,102,842 source characters mapped, all 3259 fragments located, all 2073 inspected
page anchors exact, zero missing glyphs and no failed sections. It still reports
31 placeholder graphics and one recovered package section, which remain explicit
fidelity issues. Immutable receipts are retained under
`build/parity-review/projected-line-evidence/`.

This is a source-range and picture-anchor repair. Generated empty DOM-box targets,
other CSS fidelity gaps and connected Kindle acceptance are separate gates.
