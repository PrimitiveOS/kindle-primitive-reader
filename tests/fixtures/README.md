# Reader verification fixtures

The [manifest](manifest.json) records the expected reading order, locations, links, resources, and errors. Archive paths are relative to the ZIP root. A `#fragment` identifies the XHTML element containing a known passage.

| Fixture | What to check |
| --- | --- |
| `field-notes-epub2.epub` | EPUB 2 NCX navigation, chapter order, publisher CSS, grayscale PNG, local and cross-chapter links, and multilingual text |
| `field-notes-epub3.epub` | EPUB 3 navigation, semantic footnote and back link, embedded font, and the same reading passages as the EPUB 2 book |
| `long-chapter.epub` | Continuous layout through 240 numbered observations in one chapter. Find observation 137 again after changing the viewport or font size. |
| `many-chapters.epub` | Open the first, middle, and last of 48 chapters. Check spine and table of contents order. |
| `missing-image.epub` | A valid ZIP links to a missing image. Check for usable fallback or error handling. |
| `malformed-container.epub` | A valid ZIP contains incomplete package-location XML. Check that opening fails cleanly. |
| `truncated.epub` | The ZIP has no end of central directory record. Check that opening fails cleanly. |
| `password-required.pdf` | One generated blank page. Verify the password flow rejects a wrong password and accepts `reader-test`. |

In each of the two valid Field Notes books, `tide-a` and `tide-b` contain the identical sentence `The tide keeps its own time.` Save a location at each occurrence and verify that it resolves correctly after repagination. The chapter also contains Vietnamese, right-to-left Arabic, and Japanese samples. The EPUB 3 `INK SAMPLE` line uses an embedded Silkscreen face. Use available fallback fonts for the other samples, and record their names when comparing glyph coverage.

From the repository root, use Python 3.9 or newer to regenerate or verify the EPUB fixtures.

```sh
python3 scripts/generate-fixtures.py
python3 scripts/generate-fixtures.py --check
```

The generator uses fixed timestamps and stores ZIP members without compression. It reads the pinned font and notice from this directory and runs offline. Fixture text and artwork are original and use [CC BY-NC-SA 4.0](../../LICENSE-DOCS). Silkscreen Regular retains its [SIL Open Font License 1.1](fonts/OFL-Silkscreen.txt), which is also included in the EPUB 3 archive. [Pinned Google Fonts source](https://github.com/google/fonts/tree/7e8d490955e9d2629cd3095e8dadd6fafe3c16b8/ofl/silkscreen)

[EPUBCheck 5.4.0](https://github.com/w3c/epubcheck/releases/tag/v5.4.0) reported zero errors and warnings for the four valid fixtures. For the three intentionally incomplete fixtures, it reported a missing file (`RSC-001`), invalid container XML (`RSC-016`), and a missing ZIP end header (`PKG-008`), respectively.

The password fixture was generated with pypdf 6.1.1, a 120 × 80 point blank page,
and RC4-128 encryption. Its SHA-256 is
`3d2ecdcffc6efdb74e58f660cbf017d3162d42fc08279dc4c274974633b11798`.
