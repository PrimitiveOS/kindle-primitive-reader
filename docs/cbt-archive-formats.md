# CBT comics with GNU long names

Reader's CBT index supports USTAR entries and GNU TAR long-name records. Long
UTF-8 page paths are read from the `L` record immediately before the page,
then naturally ordered with other page names. Opening a page rechecks the
archive and its name record before decoding it. Reader never extracts archive
entries onto storage.

This is a bounded candidate: names are limited to 512 bytes, an archive to 512
MiB and 8,192 entries, and images keep the existing decode limits. Absolute or
traversing paths, duplicate resolved names, missing or malformed long-name
records, links, sparse entries and other unsupported TAR extensions are
rejected. POSIX PAX headers remain unsupported. Archives can still be opened
through another reader if they use a variant outside these bounds.

KOReader v2026.07.1 registers CBT in its MuPDF provider. The pinned MuPDF TAR
reader recognizes GNU long-name records; its source does not establish PAX
support. Reader's source-level tests exercise indexing, random page decode,
natural order and invalid records; simulator and Kindle comparison have not
yet been run. [KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L374-L377) · [Reader comic implementation and tests](../src/comic.rs)
