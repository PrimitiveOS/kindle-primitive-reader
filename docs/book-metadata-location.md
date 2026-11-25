# Book metadata location

Open **Settings › Book metadata location** while reading, or **Reading data ›
Book metadata location** from the Library. Reader saves the book’s position,
per-book reading preferences, bookmarks, highlights and notes at its usual
checkpoint. Choose a location for the book’s Reader sidecar.

| Location | Where Reader keeps the sidecar |
| --- | --- |
| Book folder | A `.sdr` folder next to the book. Move the folder with the book to keep the reading data. |
| Reader profile | Reader’s private `docsettings` tree, organized by path. This is the default. |
| By edition fingerprint | Reader’s private `hashdocsettings` tree. The same edition can recover its data after a move or rename. |

[Location settings](screenshots/book-metadata-location-settings.png) ·
[About the locations](screenshots/book-metadata-location-about.png) ·
[Fingerprint metadata list](screenshots/book-metadata-location-hashed.png)

Changing the location saves an open book immediately. Other books migrate at
their next save. Reader installs the destination atomically before retiring
an older Reader sidecar for the same edition at another configured location.
A newer alternate is retained. A read-only book folder falls back to the
profile. A rejected sidecar write leaves the local reading index and prior
sidecar usable. **Save reading data** retries a pending write.

Reader keeps the live index for ordinary reading, History and collections.
When there is no live index entry for an edition, Reader uses the newest
valid matched sidecar, preferring the selected location when timestamps
match. Malformed snapshots and mismatched editions are skipped. The
[book metadata archive](book-metadata-archive.md) remains a separate
checkpoint restore option after the live locations have been checked.

**Books with fingerprint metadata** shows the stored edition titles, authors
and last known paths. Tap a book to locate the same edition in Documents
and open it. A change to the content may produce a different edition
fingerprint. Reader keeps profile directories under `.book-metadata.sdr`
in its data folder, scoped to the library index.

## KOReader comparison

The pinned native replay exercises KOReader's About and radio callbacks,
three actual DocSettings destinations, freshest-candidate selection, the
hash-based metadata list, recovery after rename and retirement of prior
metadata after a successful flush. Reader shares the three workflows, keeps
its established central index as local authority, and defaults to profile
storage for continuity with earlier Reader installations. Reader sidecars use
versioned JSON and Reader’s edition fingerprint. KOReader Lua files remain
available to Reader’s separate read-only import tools. Existing copies at
old path-based locations may remain after external file moves.
[Pinned settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/common_settings_menu_table.lua#L578-L667) ·
[Pinned DocSettings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/docsettings.lua#L61-L165) ·
[Pinned writer](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/docsettings.lua#L317-L364) ·
[Pinned hash list source](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanagerbookinfo.lua#L1005-L1028) ·
[Native hash list screenshot](screenshots/stock-book-metadata-hashed.png)

The Mac tests exercise normal settings taps, three real storage destinations,
a reordered folder, a renamed edition, saved position and bookmarks,
central versus alternate precedence, permission fallback, unreadable targets
and retry. The authored publication and neighboring KOReader sidecar retain
their source bytes. Physical Kindle acceptance remains open.
[Mac tests](../src/app/live_metadata/tests.rs) ·
[Stock replay](../scripts/koreader-live-metadata-check.lua)

The simulator test can write the three Reader screenshots using the
`READER_LIVE_METADATA_ABOUT`, `READER_LIVE_METADATA_SETTINGS` and
`READER_LIVE_METADATA_HASHED` file paths. For the stock screenshot, the
optional third argument to the Lua replay is the capture directory. The
reference capture used the synthetic case folder
`/private/tmp/reader-stock-location-fixture` at 1264 × 1680 pixels and 300 DPI.
