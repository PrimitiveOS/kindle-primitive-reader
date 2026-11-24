# About the top status header

Open **Reading status bar → About the top status header** for a short guide to
using the header in page-turn mode. The guide explains how to enable it from
**Screen** in the lower reading controls, either for the open book or as the
initial choice for books you open later. It also explains where to choose which
items appear and when continuous scrolling hides the header.

The popup follows KOReader’s separate About item and keeps the settings page
open. Reader offers **Done**, tap outside, and the physical Back page key to
close it. [Pinned KOReader About menu](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L365-L386)

The authored Field Notes EPUB workflow opened and closed the popup through the
settings row. The desktop test verifies that the book stays at the same source
position, the saved header and footer preferences retain their values, and the
footer pixels including Exit match on return to the reading page. Kindle
verification remains pending. [Desktop capture](screenshots/top-status-header-about.png)
