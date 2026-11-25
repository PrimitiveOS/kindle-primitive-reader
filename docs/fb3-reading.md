# Reading FictionBook 3

Reader opens local `.fb3` files from its Documents picker. It uses the
package's document body, title, author, language, and supported thumbnail.
Contents opens named sections at their original headings. Figures, lists,
tables, linked notes, and their return links appear in the reading flow.
Search and selection use the converted text, and reading positions reopen
against the same document. The package remains unchanged.

Reader reads ZIP parts in memory. It accepts ASCII case differences in part names,
UTF-8 path escapes, extensionless parts declared in the package, and
root-relative and part-relative image relationships. Missing or unsupported
images can use their supplied alternate text. External image relationships
are skipped. Ordinary HTTP and HTTPS text links use Reader's regular link
controls. [FictionBook 3 body schema](https://github.com/gribuser/FB3/blob/92bdc0d1fe89d9ac122ad342fd3c4cdc81a11c24/fb3_body.xsd) ·
[Pinned CREngine FB3 adapter](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/fb3fmt.cpp#L5-L108) ·
[Reader package importer](../src/fb3.rs)

A large publication can use multiple reading sections. Reader keeps whole
sections when they fit. It splits oversized sections, notes, and supported
block containers between complete children. Each continuation reopens its
surrounding structure so language and formatting still apply. Headings and
IDs appear once, and Contents, notes, and backlinks point to the section
containing the original destination. Planning first preserves the existing
384 KiB text grouping. If converted markup exceeds its output limit, Reader
retries with a 704 KiB source-markup planning budget. An indivisible block
such as one paragraph or table that exceeds its budget reports a size error.
Other limits include a 16 MiB ZIP, 512 parts, a 2 MiB body XML file, 48 levels
of nesting, 1 MiB of converted markup per group, at most 2 MiB per image,
and an 8 MiB image budget. The supported
media are PNG, JPEG, BMP, first-frame GIF and static SVG. For supported simple
WebP still images, Reader now snapshots a bounded grayscale PNG in memory under
the same FB3 resource key so the reflow renderer can display it; the source
archive, generated XHTML and anchors remain unchanged. Unsupported or
over-budget WebP keeps the previous fallback and may remain blank. Actual
rendering, resource usage and Kindle behavior remain unverified.
Specialized FB3 typography, multimedia, embedded interactive elements, and
KOReader reading-position import for FB3 remain outside this implementation.
[Section grouping and links](../src/fb3/body.rs)

## In-page footnotes

Open **Style tweaks › In-page FB3 footnotes** while reading an FB3 book.
The book choice and all-books default use Reader’s existing FictionBook
footnote setting. Fresh Reader settings leave it off. **Keep regular FB3
note size** changes the 75% band text to the reading size. The original note
keeps its document position. Backlinks, selection and saved locations in the
band use the same text as the original.

Band layout applies to supported single-page horizontal reading and links to
notes within the same converted part. A single, complete text-only note in a
different part can also appear in a verified blank area on the reference page.
Notes that span parts and other unsupported cases remain reachable through
their original links. Existing page, note and reference limits apply.
Facing pages, continuous reading and unsupported note layouts keep the ordinary
reading flow. [Reader pagination](../src/footnotes/in_page.rs) ·
[FictionBook 2 controls](fb2-footnotes.md#in-page-footnotes-and-endnotes) ·
[Cross-part FB3 notes](fb3-cross-part-notes.md)

KOReader converts FB3 `<notes>` to a FictionBook `notes` body and `notebody`
to `section`. Its `fb2-document` selector matches both FictionBook formats.
The EPUB setting and separate FB2 comments setting leave the FB3 fixture in
the ordinary flow. KOReader enables FictionBook footnotes in a fresh profile.
At 720 × 480, the note separator appears at y=411 at 75% and y=360 at regular
size. The word `midnight` measures 78 px and 104 px in the original note.
The reference stays on page 2 and the original text and backlink stay on
page 13 with identical source pointers. [Pinned FB3 conversion](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/fb3fmt.cpp#L165-L223) ·
[Pinned format selector](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L3484-L3497) ·
[Pinned note settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L851-L918)

[KOReader 75% note](screenshots/stock-fb3-note-on.png) ·
[KOReader regular size](screenshots/stock-fb3-note-regular.png) ·
[KOReader original note](screenshots/stock-fb3-note-original.png)

[Reader note off](screenshots/reader-fb3-note-off.png) ·
[Reader 75% note](screenshots/reader-fb3-note-on.png) ·
[Reader regular size](screenshots/reader-fb3-note-regular.png) ·
[Reader original note](screenshots/reader-fb3-note-original.png) ·
[Reader settings](screenshots/in-page-fb3-style.png) ·
[Reader settings in dark mode](screenshots/in-page-fb3-style-dark.png)

For the native comparison, use the [in-page replay](in-page-footnotes.md#replay-on-macos)
with `tests/fixtures/harbor-survey.fb3` as the first argument and `fb3` as the
fifth. Run `cargo test --locked --features kindle --lib fb3_in_page_footnotes`
with `READER_IN_PAGE_FOOTNOTE_CAPTURES` and `READER_SELECTION_VIEWPORT_CAPTURES`
pointing to existing directories to capture Reader’s note and settings frames.

## Desktop comparison

The pinned KOReader rendered the format author's unchanged Anatomy sample,
part-name stress sample and large reading sample in 32, 142 and 296 pages
respectively. The sample archives stay external to the Reader repository.
The harness also opens *Harbor Survey*, an original fixture, and checks its
two named headings and two searchable passages. [Upstream examples](https://github.com/gribuser/FB3/tree/92bdc0d1fe89d9ac122ad342fd3c4cdc81a11c24/Examples) ·
[Native comparator](../scripts/koreader-fb3-check.lua) ·
[Authored fixture](../tests/fixtures/harbor-survey.fb3)

Reader opened all three from the normal Mac file picker, painted the next
page after a Page Forward event and restored it through Continue reading.
The large sample initially exceeded the single-chapter limit. Its three
bounded sections all painted after grouping. The source archive SHA-256
values matched before and after both readers ran.

| Original upstream sample | Reader sections | Reader first frame on Mac | KOReader pages at 650 × 900 |
| --- | ---: | ---: | ---: |
| Anatomy tutorial | 1 | 2,167 ms | 32 |
| Part-name stress sample | 1 | 4,926 ms | 142 |
| Large reading sample | 3 | 4,313 ms | 296 |

Both readers show the book's paragraphs, emphasis, lists, figures, and
table cells. Reader also paints the original section headings and uses the
alternate text where the example refers to an external image. The captures
show each reader's first page of the book's text. [Reader capture](screenshots/reader-fb3-harbor-survey.png) ·
[KOReader capture](screenshots/stock-fb3-harbor-survey.png)

The longer continuation fixture has a nested section over the former
single-section limit. Both readers find its headings and follow links in
both directions across the split. These captures show the late passage,
with each reader's own pagination and typography. [Reader continuation](screenshots/reader-fb3-harbor-continuation.png) ·
[KOReader continuation](screenshots/stock-fb3-harbor-continuation.png)

Run `cargo test --locked --features kindle --lib fb3` for the package,
picker, grouping, and rendering regressions. The separate originals test
uses `READER_FB3_ORIGINALS` with the three unchanged packages named
`anatomy.fb3`, `part-names.fb3`, and `nightmare.fb3`. Set
`READER_FB3_CAPTURES` to an existing directory for the opening and
continuation screenshots. Pass both authored `.fb3` paths to the native
comparator to capture the corresponding KOReader pages.
[Reader tests](../src/fb3/tests.rs)
