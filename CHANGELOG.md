# Changelog

## 0.1.20

- Honor publisher page breaks and requests to keep paragraphs or adjacent blocks together in horizontal reflow.
- Suggest similar dictionary headwords after a spelling or word-form lookup misses. Close the popup at any time to return to reading.

## 0.1.19

- Look up inflected English words in installed dictionaries. Set a library default or change the preference for the current book.
- Read below the status bar in reflowable and fixed-page books, with compact controls to show reading status or exit.
- Recover inline images and internal links in DRM-free legacy MOBI books, while preserving earlier saved text locations. Supported unencrypted hybrid books retain their legacy edition and can use their secondary KF8 content if needed.
- Browse up to 5,000 PDF and DjVu search results, jump to the first or last page of results, and cancel a scan while it runs.

## 0.1.18

- Set left and right margins separately while reading, or link the top and bottom margins.
- Jump directly to the first earlier or last later search match in a PDF or DjVu book. Case-insensitive search recognizes compatible Unicode forms and highlights the original word.
- Read the publisher's underlines, overlines and strike-throughs in EPUBs. Links can keep their original appearance.

## 0.1.17

- Open oversampled CMYK print photographs faster at page fit. Detailed zoom uses the full decoder.
- Read raised footnote numbers and powers, and lowered subscripts in EPUBs. Links and bookmarks still point to the original text.
- Show the estimated time left in the current chapter or book using the pace recorded for that book.
- Use spaces at either end of a PDF search to match word boundaries. Saved searches remember those spaces.

## 0.1.16

- Generate stable pages for an EPUB from 500 to 3,000 characters per page. Jump to a stable number, show it in the margin or footer and set defaults for new books.
- In PDF and DjVu, find matching pages forward or backward from the page you are reading. Returning restores the starting page and viewport.
- Read supported publisher fonts in KF8 and in EPUBs that use standard IDPF font embedding.

## 0.1.15

- Find up to 5,000 occurrences in reflowable text. Move to the first or last page of results, open the passage and return to your reading position.
- Use the publisher’s embedded regular, italic, bold and bold-italic faces in supported EPUBs.

- Browse the publisher’s print page labels, jump to an exact label, and show print boundaries in the margin or footer of a reflowable EPUB. Each book remembers its choices.

## 0.1.14

- Read publisher GIF illustrations inline and open them at full size. The first frame is shown for animated GIFs.
- Find publisher apostrophes, typographic dashes and spaces using the ordinary search keyboard.

- Choose which offline dictionaries to use and their order. A book can keep its own preferred sources, including one disabled by default.
- Find text in reflowable books with regular expressions. Navigate matches from the current passage and recover the source text across authored soft hyphens.
- Set a word’s preferred line breaks from its selection. Rules follow the language across books and can be changed, disabled or removed.

## 0.1.13

- Tap a saved highlight to read its note on the page. Choose between overlapping marks, page through long notes and return to the same location.
- Find the next or previous occurrence from the current passage in a reflowable book, including matches beyond the first 100 results.
- Look up words using the StarDict dictionaries already installed with KOReader. Direct entries and synonyms can appear together. XDXF entries show the pronunciation in brackets. Reader also searches its own dictionary folder and remembers the preferred dictionary.

## 0.1.12

- Preview and import page bookmarks, exact highlights and their notes from KOReader metadata beside an EPUB. Repeating the import keeps one copy of each mark.
- Honor publisher first-line and hanging paragraph indents, including an explicit zero reset. Paragraph wrapping, selection and bookmarks follow the same source text. Chapter-opening paragraphs follow supported sibling and first-of-type stylesheet rules.
- Find a PDF phrase across a printed line break within a column. The search result highlights both lines and opens the passage in the current view.
- Tap a PDF link to preview the address, copy it into a note or search, or scan an offline QR code.

## 0.1.11

- Preview a KOReader reading position saved beside the open book. Select Apply to resume there and use Back to return.
- Extend a saved text highlight across chapters or a fixed-page highlight across pages. Cancel restores the original passage. Applying keeps the note with the extended selection.
- Browse publisher-provided chapter headings in bundled DjVu books through Contents and Book map.

## 0.1.10

- Open Book map to see a fixed document's pages, chapters, bookmarks and highlights. Browse a page group with the Oasis buttons or tap a page to read it. The map and footer follow your choice of publisher or custom chapters.
- Search bookmarks, highlighted passages and notes together. Filter by annotation type or match case. Open a matching passage from either reading bar.
- Keep the selected passage visible when opening a PDF search result or saved highlight at high zoom. Location history restores the earlier page and viewport.
- Respect publication language in supported Kindle books and British English in individual language-tagged passages. Search, selections and saved reading positions use the original book text.

## 0.1.9

- Open Contents at the section you are reading from either reading bar. Return to the same passage after checking your place in a long outline.
- Follow a PDF contents link to its specified position in a zoomed or continuously scrolling page. Back and Forward restore the previous view.
- Use automatic American English hyphenation in eligible text. Search, word selection and bookmarks retain their original positions when words wrap across lines.

## 0.1.8

- Read numbered and bulleted lists, including nested lists, reversed numbering and publisher-specified starting values.
- Keep table entries under their headings in supported EPUB and Kindle books, including tables with percentage column widths and merged cells. Select text and follow links in individual cells.
- Return directly to the last painted page of a chapter when extra line breaks or fractional line spacing follow its final text.
- Use installed Kindle font fallbacks for additional mathematical signs and symbols.

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
