# Search over styled reading text

This staged slice shares the chapter projection used by the renderer migration.
It does not enable user `display`/`visibility` input or claim completed renderer
parity. Fixed-page search remains a separate provider path.

Whole-book literal/regular-expression search, directional search and their
snippets use projected text. `display:none` content is excluded, including when
a descendant declares itself visible. `visibility:hidden` text retains geometry
and remains searchable, following the pinned CREngine comparison. Match anchors
keep canonical source offsets; each result also retains the disjoint source
intervals across a suppressed gap. Copying the envelope between the first and
last interval would incorrectly include hidden text.

The immutable font-free context contains the publisher-style policy and the
effective user-style snapshot. Stylesheets use the same loader, caps, inline
rules and cascade as layout, without registering or reading font-face payloads.
Chapter extraction uses `Book::chapter_source` and prior-MOBI compatibility, so
publication language, numeric history and synthetic Kindle targets agree with
the reading provider.

The application request key includes edition and publication identities as well
as the full style snapshot. Both in-flight deliveries and retained completed
results are checked before use; changing styles cancels the request and clears
obsolete matches. Closing a settings panel does not erase the key while matches
still exist. The `Action::SearchHit` integration guard is owned by the renderer
consumer slice and must be assembled before release.

Result memory accounting includes the retained source intervals. Existing query,
chapter, result-count, cancellation and mailbox bounds remain. Source parsing
and font-free stylesheet loading occur in the worker's storage lease; matching
occurs after it is released. Native USB timing remains an acceptance gate.

Eight new regressions cover hidden joins in literal/regex/directional matching,
visibility and ancestor suppression, Unicode/accent boundaries, configuration
and publication keys, actual worker cancellation and retained-result invalidation.
They are authored for the staged branch; compilation/execution is recorded by
its CI and later combined Mac runs. Original-file acceptance and native behavior
must remain separately identified.
