# Automatic page-turn parity

## KOReader reference

Compared against the checked out KOReader `v2026.07.1` source at commit
`9192014d8bd82a91dc1012473be0f238dedfdb54`, specifically
[`autoturn.koplugin/main.lua`](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/autoturn.koplugin/main.lua).
The plug-in stores the timeout, enabled state and movement distance. It starts
its clock on activation, resets the inactivity point on input, and advances one
relative view only when Reader is the top visible widget. It unschedules on
suspend or close and starts a fresh timeout on resume. Its timeout picker accepts
up to one day; a held menu action configures the signed fractional scroll
distance. The setting spans −20.0 to +20.0 page heights, changes in 0.1
increments, and changes in 0.5 increments while held. [The paging module]
(https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerpaging.lua)
maps this distance to continuous scrolling or relative page movement. Its
fixed-page pan advances by 60% of the visible area using the default 40% zoom
overlap and the configured reading direction. [Zoom overlap and direction
defaults](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerzooming.lua#L68-L83) · [Fixed-page movement](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L957-L1084)

## PrimitiveOS Reader implementation

The Navigation group offers the existing presets (Off, 10, 30, 60 and 120
seconds), a **Custom interval** keyboard entry and a **Scroll distance**
control. Custom intervals accept 1 to 86,400 seconds. Distance moves in 10% or
50% steps within −2,000% to +2,000% of a visible page. Both preferences are
stored in `reading-controls.json` and restored when another book opens or
Reader restarts. **Off** disables automatic turns but keeps the last interval
for the next time they are enabled. User input resets the deadline; a late
scheduler tick performs one movement without catching up. Menus and other
non-reading screens pause the timer, and Kindle Sleep pauses it until Wake.

In continuous EPUB and PDF views, the selected distance moves that fraction of
the content viewport and negative values reverse direction. Paginated EPUB
turns whole pages, rounding any nonzero distance away from zero. Noncontinuous
fixed pages pan by 60% of the visible area, matching KOReader's default 40%
overlap. Reader pans horizontally first, reverses that axis for right-to-left
books, then advances vertically at the row edge; it turns or skips pages after
reaching the final visible area. The same rule works for portrait and landscape
pages.

The fixed-page implementation currently uses KOReader's default overlap and
horizontal-first direction. Reader does not yet expose KOReader's configurable
overlap percentage, vertical-first column direction or bottom-to-top reading
direction. Those controls remain tracked separately from the default AutoTurn
movement.

The interval is global to Reader, as in KOReader. Device-level frontlight,
rotation and sleep behavior remain owned by Primitive Launcher and the app kit.

## Desktop evidence

![Automatic page-turn settings in the Reader simulator](screenshots/auto-turn-settings.png)

The functional test `custom_auto_turn_interval_is_validated_saved_and_restored_on_reopen`
exercises the settings row, 1..86,400 validation, persistence, reopening the
book, sleep/wake pause, and return to the last book. A second test exercises the
distance controls and persistence. Session tests cover signed fractional EPUB
scroll, fixed PDF scrolling and multi-page movement. The `AutoTurn` unit tests
cover activity resetting the deadline, paused timer behavior, disabling, and a
late tick producing exactly one turn. The simulator functional test captures
the screenshot above. The focused interval test also passes independently.
`cargo test --locked` passed all 901 library tests with 66 ignored, the Kindle
binary test (1), EPUB package tests (14), and layout probes (4). Documentation
tests pass with the explicit `RUSTDOC` path used in this Mac environment. The
new fixed-page tests cover horizontal and vertical movement, RTL direction,
signed reversal, overlap distance and page-edge transitions. The ARMHF build for
this candidate and Kindle installation are still pending.

## Remaining parity work

- Add per-book and global controls for fixed-page pan overlap and direction.
- Compare USB handoff and actual sleep/resume timing on the Kindle. This Mac simulator check does not validate physical panel refresh or deep-suspend timing.
- Device acceptance for the current Reader build is pending.
