# Reflow Book Map reading time

The reflow Book Map can show recorded reading time beside a chapter. New visits
record an optional spine index and the Reader publication revision alongside the
existing edition ID and opaque page key. A row counts only visits for the open
edition and matching publication revision whose chapter index is still valid.
The existing visit durations, total statistics, page keys and reading position
are unchanged. When a dated visit belongs to the current session, the row says
**Includes this session**; its total can also include earlier sessions.

Older and imported visits have no verified chapter attribution and are never
guessed from their irreversible page keys. When such visits coexist with mapped
visits for the book, the displayed duration is labeled **Reading time
(partial)**. Only visits accepted by the existing statistics settings are
recorded, and disabling statistics hides the chapter times. If no visits have
been attributed, no chapter time is shown. The aggregate statistics remain
available independently.

The optional field is omitted from legacy journal records, preserving their
existing serialization. A single pass over the bounded 100,000-visit journal
collects totals; there is no source-file access or layout work during the map
draw. If new optional attribution would alone exceed the 16 MiB journal limit,
Reader keeps the newly recorded visits and their timing while omitting only the
new uncommitted attribution. Existing committed visits and chapter metadata are
not discarded. If the journal is still too large, the normal save failure and
rollback apply. This makes the map incomplete at the limit, but does not
silently remove reading history.

KOReader's [Book Map statistics integration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L825-L833)
and [current-session shading](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L373-L395)
motivate this chapter-level presentation. Exact per-page bars, historic
attribution and physical behavior remain open. This implementation has not yet
been validated in simulation or on Kindle.
