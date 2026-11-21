# Changelog

## 0.1.7

- Read long EPUB paragraphs whose embedded fonts previously prevented page breaks. Search the selected words directly from the reading page.
- Keep illustrations visible within the Kindle image-memory budget, and restore the exact page when a paragraph contains several full-page pictures.
- Preview an EPUB footnote over the page, look up a word inside it, or jump to the full note and return.
- Switch dictionaries and results, find text within a definition, and open nested lookups with the original selection and scroll position preserved.
- Reduce first-page drawing time and peak memory on print-color PDFs. The large original PDF's rendered cover matched the previous release pixel for pixel on the Oasis.
- Read JPEG 2000 images in Documents and inside comic archives, including tested grayscale, RGB and subsampled color examples.

## 0.1.6

- Open full-length MOBI and KindleGen AZW3 books with separate stylesheet and artwork flows. Keep links usable in large illustrated chapters.
- Improve compatibility with EPUB cover pages, XML declarations, ZIP packaging and filenames found in Kindle Documents.
- Preserve the publisher’s order in right-to-left spreads. Add custom contents entries, book and chapter progress, and configurable reading gestures.
- Add fine scan straightening and optional OCR over embedded text in PDF and DjVu. Include the offline English OCR engine and model in the Kindle package.
- Add Wikipedia and translation for selected text and explicit page lookups, with separate controls for online history and language.
- Remember the last reading panels separately for each book. Search can ignore accents while keeping the original word selected.
- Open an extracted EPUB as a book, or choose an individual chapter from Documents. Protected AZW and KFX entries identify the Kindle Library as their reader.

## 0.1.5

- The package declares firmware 5.18.2 (the firmware the Reader is tested on), so the Store lists it with the same range.

## 0.1.4

- The release workflow names the package (`reader`), so the Kindle binary is packaged. The 0.1.3 run failed on that step and published nothing.

## 0.1.3 (not published)

- Follows the system layer under Primitive Launcher: the theme is the system's, and the power listener survives the redraw signal (the app kit pin moves to 67db0b5).
- A dark icon for the library, staged as `# IconDark:`.
- The button that leaves the reader says Exit.
- The Kindle build is on by default, so the release package builds it.

## 0.1.2

- Released before this changelog.
