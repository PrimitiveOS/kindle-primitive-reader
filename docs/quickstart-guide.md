# Reader Quickstart guide

While reading, open the top toolbar and choose **Settings › Quickstart guide**.
From Documents, choose **Controls › Quickstart guide**. Tap a topic or use the
on-screen pager, page buttons, or a horizontal swipe. **Topics** returns to
the guide index. **Done** returns to the book, page, and menu you were using.
[Guide topics](screenshots/quickstart-topics.png) ·
[Reading-view topic in dark landscape](screenshots/quickstart-reading-view-landscape-dark.png)

The guide covers opening and resuming EPUB and PDF files, navigation and search,
bookmarks and notes, text and PDF layouts, offline StarDict, and exiting Reader.
It is compiled into Reader and available offline. A fresh profile shows it once
when Reader starts from Launcher. A direct document open starts with that
document. An existing Reader profile can open the guide from the menu.

KOReader opens a generated local HTML guide through its normal reader from
**Help › Quickstart guide**. On an ordinary launch, it checks the last-seen
guide version. Its explicit document and folder launch paths skip that check.
Reader presents its own controls and keeps the source menu and book open under
the guide. [Stock Help callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/common_info_menu_table.lua#L32-L39) ·
[Stock launch paths](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/reader.lua#L243-L266)

The pinned Mac replay invokes the Help callback, checks the local HTML and
linked topics, verifies that it marks the guide as shown, and records the file
handed to the stock reader. The Reader simulator exercises the real menu and
guide controls in EPUB, PDF, and Documents and checks that guide visits leave
recent books and saved progress intact. [Stock replay](../scripts/koreader-quickstart-check.lua) ·
[Reader tests](../src/app/quickstart/tests.rs)

Run `cargo test --locked --features kindle --lib quickstart_guide`. Set
`READER_SELECTION_VIEWPORT_CAPTURES` to an output directory to write the Reader
screenshots.
