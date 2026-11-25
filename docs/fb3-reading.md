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

A large publication can use multiple reading sections. Reader groups
complete top-level sections to stay within a 384 KiB original-text budget,
and keeps linked notes, Contents and backlinks mapped across the resulting
chapters. A single top-level section above that budget reports a size error.
Other limits include a 16 MiB ZIP, 512 parts, a 2 MiB body XML file, 48 levels
of nesting, 1 MiB of converted markup per group, at most 2 MiB per image,
and an 8 MiB image budget. The supported
media are PNG, JPEG, BMP, first-frame GIF, static SVG and supported WebP.
Specialized FB3 typography, multimedia, embedded interactive elements, and
KOReader reading-position import for FB3 remain outside this implementation.
[Section grouping and links](../src/fb3/body.rs)

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

Run `cargo test --locked --features kindle --lib fb3` for the package,
picker, grouping, and rendering regressions. The separate originals test
uses `READER_FB3_ORIGINALS` with the three unchanged packages named
`anatomy.fb3`, `part-names.fb3`, and `nightmare.fb3`. Set
`READER_FB3_CAPTURES` to an existing directory for the authored screenshot.
[Reader tests](../src/fb3/tests.rs)
