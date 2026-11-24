# Top status header font size

Open **Reading status bar → Header font size** to change the size across books.
The picker starts at your saved size, or 20 px on a fresh profile. Tap − or +
for a 1 px change, hold for 4 px, or use the page buttons for 1 px. Tap the
number to enter a whole size from 8 to 36 px.

**Default: 14 px** sets the draft. **Set size** saves the draft and leaves the
picker open. **Close** or tapping outside the picker discards edits made since
the last save. The book’s top header activation stays in its reading state.

The pinned KOReader picker also starts at 20, uses 14 for its Default button,
steps without wrapping, accepts direct numeric input and stays open after Set
size. Reader uses the same integer controls with its shared keyboard.
[Header settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L527-L548) · [Spinner defaults and page keys](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/spinwidget.lua#L26-L80) · [Typed entry and bounds](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/numberpickerwidget.lua#L128-L212)

The desktop test opens the picker through its Settings row, exercises taps,
holds, page keys, typed entry, bounds, default, close and persistence across
restart. It compares the reading frame before and after applying 36 px. The
header pixels change while every pixel below the header, including the reading
footer and its Exit button, is identical. Kindle panel verification is pending.

[Font picker](screenshots/top-status-header-font-picker.png) · [36 px reading header](screenshots/top-status-header-font-large.png)
