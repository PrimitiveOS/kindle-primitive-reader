# Saved reflow position recovery

When an existing reflow book has a saved reading position, Reader stages its
saved chapter and location before switching away from the current book. If the
chapter cannot be opened or a saved source anchor cannot be resolved in the
current edition, the open fails with an explanation and the saved position is
kept. Reader does not silently replace it with the beginning of the book.

A valid source anchor remains authoritative when its text is temporarily hidden
by a stylesheet. Older page-only records retain their existing page fallback and
may be clamped to the available pages. If an automatic reading profile cannot
preserve an anchor, Reader retries the saved layout instead of applying that
profile. A later explicit reading action can update the position normally.

The outgoing book stays open after a failed switch. The existing switch path
still closes a temporary PDF search-fit view before preparing the new book,
so that transient view and its directional search state may be cleared even
when the switch fails. A viewport change may also decline a CSS-hidden focus
if it cannot place that anchor visibly; the saved position is retained.

This is an implementation candidate. Real-book, simulator and Kindle behavior,
including unavailable chapters, hidden passages and automatic profiles, still
requires verification.
