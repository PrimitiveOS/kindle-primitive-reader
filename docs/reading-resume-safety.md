# Saved reflow position recovery

When an existing reflow book has a saved reading position, Reader stages its
saved chapter and location before switching away from the current book. If the
chapter cannot be opened or a saved source anchor cannot be resolved in the
current edition, the open fails with an explanation and the saved position is
kept. Reader does not silently replace it with the beginning of the book.

For an ordinary packaged EPUB, if opening the saved position fails but the
beginning can be staged, Reader offers **Open beginning**. The confirmation
explains that it replaces the saved position; Cancel leaves it unchanged. Reader
rechecks the book identity, source stamp and saved position, stages the beginning
again, and persists its position before switching books. A failed check or save
leaves the old position and annotations intact. Folder books, raw-whitespace
positions and migration-dependent openings do not use this recovery path.
The stamp is metadata-based and relies on the supported single-writer storage
model. This is an unverified implementation candidate.

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
