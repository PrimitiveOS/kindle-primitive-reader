# Searchable contents

Open **Contents** from the upper Navigation menu or lower reading bar. Publisher
outlines start with their branches collapsed. Reader opens the branches that
lead to the current heading and other headings on the same page. A left
chevron expands or collapses the branch. The heading beside it still opens
its original EPUB fragment, fixed page, or saved custom location.

**Search** uses the shared keyboard to find heading text without matching case.
Results keep their ancestor headings for context. A single `*` expands the
whole outline. **Clear** restores the full list and reveals the current
heading again. Cancelling or entering an unmatched query keeps the previous
view. The physical page buttons, on-screen buttons, and vertical swipes page
the list without changing the book position. Only choosing a destination adds
to navigation history.

The generated and custom outline entry points stay available. Searching uses
whichever publisher or custom outline is active. [Contents list settings](contents-list-settings.md)
set the number of visible rows and the size of the entry font. Chapter lengths,
page labels, dot leaders, and per-depth progress ticks remain open.

## Evidence

The authored EPUB test opens a nested heading, collapses a branch, uses the
physical page button input, types a query, and verifies cancel, no-results,
and clear. Its final tap follows the original fragment and leaves one return
entry. The authored PDF test checks the same split between branch controls and
page navigation. Existing EPUB, MOBI, KF8, and DjVu fixtures also exercise
provider-specific targets and restore the original reading frame.

| Collapsed | Current path expanded | Title results |
| --- | --- | --- |
| <img src="screenshots/contents-tree-collapsed.png" alt="Collapsed Contents with chapter branches and page count" width="260"> | <img src="screenshots/contents-tree-expanded.png" alt="Contents opens the current chapter and displays its child headings" width="260"> | <img src="screenshots/contents-tree-filtered.png" alt="Search results preserve parent chapters and show Search and Clear" width="260"> |

The full-screen simulator captures use the authored EPUB. The foreground
Kindle and physical E Ink behavior remain unverified for this change.

[Reader list and input](../src/app/contents_view.rs) · [Tree and title matching](../src/app/contents_tree.rs) · [EPUB and PDF workflow tests](../src/app/contents_view/tests.rs) · [KOReader source](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L770-L1182)
