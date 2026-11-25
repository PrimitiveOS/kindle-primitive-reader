# Book metadata archive

Open **Settings › Book metadata archive** while reading or **Reading data › Book metadata archive** from the Library. Choose an existing folder in Documents. Reader writes `primitive-reader-book-metadata.json` there. When you choose a different folder, earlier copies stay in their original folder. Select that folder again to see them.

<img src="screenshots/book-metadata-archive-settings.png" width="390" alt="Book metadata archive settings with a folder selected and save on close enabled">

**Save metadata on book close** is off until you enable it. It saves a copy when you exit, return to the Library or successfully switch books. Sleeping or running a manual save writes the normal reading state. **Save metadata before deleting** starts on after you choose a folder. For a deletion from Reader, the archive write must succeed before Reader removes the file. Reader still keeps its regular records when you turn archive saving off.

**Archived books** shows the title, checkpoint date, whether it came from a close or deletion, and the saved bookmarks, highlights and notes. Tap a mark for a read-only preview. Removing one saved copy leaves the book and its regular reading data in place.

<img src="screenshots/book-metadata-archive-list.png" width="390" alt="Two books listed with the date and reason their reading data was archived">

When you open the same edition in a profile without a live reading record, Reader offers to restore its reading position, layout settings and marks. The file can have a different name or be in a different folder. **Restore** saves the recovered reading record before removing the archived copy. **Start fresh** opens the book and saves a new reading record before discarding the archived copy. The physical back button cancels opening and leaves the archive available. If the saved copy changes while the prompt is open, Reader keeps the newer copy.

<img src="screenshots/book-metadata-archive-restore.png" width="390" alt="Reader offers to restore position, settings and marks or to start fresh">

## KOReader comparison

The pinned native KOReader replay uses its actual `DocSettings` writer, archived list and `ReaderUI` opening callbacks. It checks optional close and deletion checkpoints, an archive entry’s note and reading position, same-edition lookup after a rename, live metadata taking priority, and the Restore and Discard choices. Reader uses its own edition identity and JSON; the native archive cannot read KOReader’s Lua archive files. KOReader moves entries when changing the destination. Reader leaves earlier copies at the folder where they were created and restricts destinations to Documents. Reader exposes saved marks in its archived book details, with removal applying to the selected copy. [KOReader archive settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/common_settings_menu_table.lua#L695-L760) · [KOReader archive writer](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/docsettings.lua#L609-L649) · [KOReader opening callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L634-L664)

Reader checks the three [live book metadata locations](book-metadata-location.md)
before offering to restore an archive. The three KOReader sidecar locations
also support the existing [read-only custom metadata lookup for profiles](reading-profiles.md#apply-a-profile-on-book-opening).

The Mac simulator checks the workflow through touch controls and normal document opening. It switches from EPUB to PDF, archives on exit, recovers an EPUB’s exact anchor and bookmark after a rename, starts fresh, blocks PDF deletion when the archive cannot be written, preserves live marks when removing an archived copy and keeps a newer archive written while the restore prompt is open. The tests use authored documents and separate live library records.

To repeat the stock comparison, run from the pinned KOReader runtime with an empty absolute scratch folder and the Reader repository path:

```sh
KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-book-metadata-archive.lua" \
  "$scratch" "$reader/tests/fixtures/field-notes-epub3.epub"
```
