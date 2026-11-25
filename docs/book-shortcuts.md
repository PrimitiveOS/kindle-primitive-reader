# Book shortcuts

Open **Tools › Book shortcuts** in a book or **Organize › Book shortcuts**
from the Library. Use **New shortcut** to choose a readable file. For a folder,
open it in the picker and tap **Add this folder**. The list shows the kind
and containing folder so identical filenames can be distinguished.
[Reader shortcut list](screenshots/book-shortcuts.png) ·
[Documents picker](screenshots/book-shortcut-picker.png)

A book shortcut opens the file through Reader's normal opening path and
resumes any saved location. Folder shortcuts share a default action. Choose
**Browse folder** to show its Documents list or **Open last-read book** to
resume its most recent available edition. Enable **Include subfolders** to
extend that search to true descendants. Hold a shortcut to remove it
with confirmation. The original file and reading records stay in place.
[Folder action](screenshots/book-shortcuts-folder-action.png) ·
[Removal confirmation](screenshots/book-shortcut-remove.png)

Saved shortcuts also appear among the actions for assigned gestures and
multiswipes. Removing one leaves its previous assignment marked as removed
until it is reassigned. Adding a new shortcut creates a separate assignment
identity, even if it points to the same book. A missing target leaves the
current book open and shows its availability in the shortcut list.

## KOReader comparison

The pinned plug-in exposes a chooser, direct file and folder opening,
last-read and recursive options, persistent settings, and hold-to-remove.
The native Mac replay invokes the actual chooser and menu callbacks, runs
the routing logic against an isolated test history, verifies saved Lua
settings and captures the folder options. The Reader simulator opens the
test EPUBs through its normal provider and checks reopening, source
preservation and assigned gestures.
[Pinned plug-in source](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/bookshortcuts.koplugin/main.lua#L35-L220) ·
[Stock replay](../scripts/koreader-book-shortcuts-check.lua) ·
[Stock folder action](screenshots/stock-book-shortcut-folder-action.png) ·
[Reader tests](../src/app/book_shortcuts/tests.rs)

Reader keeps shortcuts within Documents. In recursive mode it compares
path components, so a similarly named neighboring folder is outside the
search. The chooser presents supported documents and folders. Stock script
execution and its general plug-in dispatcher are outside the reading scope.
Foreground Kindle acceptance remains open.
