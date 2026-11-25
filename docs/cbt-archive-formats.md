# CBT comics with GNU long names

Reader's CBT index supports USTAR entries, GNU TAR long-name records, and a
bounded subset of local POSIX PAX headers. Long UTF-8 page paths are read from
the immediately preceding GNU `L` or PAX `x` record, then naturally ordered
with other page names. PAX supports `path` and optional `mtime`, `atime` and
`ctime` fields, including timestamp-only records; it rejects duplicate fields,
global records and any other attributes. Opening a page rechecks the archive
and its associated extension record before decoding it. Reader never extracts
archive entries onto storage.

This is a bounded candidate: names are limited to 512 bytes, an archive to 512
MiB and 8,192 entries, and images keep the existing decode limits. Absolute or
traversing paths, duplicate resolved names, missing or malformed long-name
records, links, sparse entries and other unsupported TAR extensions are
rejected. PAX metadata is limited to 16 KiB and 64 records per entry; size
overrides and other fields are unsupported. Archives can still be opened
through another reader if they use a variant outside these bounds.

KOReader v2026.07.1 registers CBT in its MuPDF provider. The pinned MuPDF TAR
reader recognizes GNU long-name records; its source does not establish PAX
support. The PAX implementation, random-access validation, malformed input
handling and simulator/Kindle comparisons remain unverified; KOReader PAX
parity is not established. [KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L374-L377) · [Reader comic implementation](../src/comic.rs)
