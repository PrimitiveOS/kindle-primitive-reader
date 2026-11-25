# Reading-size and mode gestures

In **Reading controls → Reading gestures**, assign **Increase font size**,
**Decrease font size**, or **Toggle pages and scrolling** to an available
corner, edge, diagonal or multiswipe gesture. Font actions use the same
half-step, pending-size handling, saved per-book preference and anchored reflow
as the normal typography panel. The mode action switches between pages and
continuous scroll using the existing per-book setting. These actions leave
fixed-page PDFs and other non-reflowable documents unchanged. A vertical-text
book also ignores the mode switch, as in the current typography controls.

The pinned KOReader dispatcher exposes increase/decrease font size in half
steps and a reflow view-mode action. Reader uses its own gesture assignment
interface for those actions. This is an unverified candidate: targeted
persistence, gesture-dispatch and page-identity tests have been added but not
run; stock runtime, simulator and Kindle checks remain pending. [KOReader
font actions and view mode](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L240-L277) · [Reader gesture dispatch](../src/app/gestures.rs)
