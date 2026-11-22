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
distance.

## PrimitiveOS Reader implementation

The Navigation group offers the existing presets (Off, 10, 30, 60 and 120
seconds) and a **Custom interval** keyboard entry. Custom intervals accept 1 to
86,400 seconds. The selected interval is stored in `reading-controls.json` and
restored when another book opens or Reader restarts. Zero disables turns. User
input resets the deadline; a late scheduler tick performs one movement without
catching up. Menus and other non-reading screens pause the timer, and Kindle
Sleep pauses it until Wake.

The interval is global to Reader, as in KOReader. Device-level frontlight,
rotation and sleep behavior remain owned by Primitive Launcher and the app kit.

## Desktop evidence

![Automatic page-turn settings in the Reader simulator](screenshots/auto-turn-settings.png)

The functional test `custom_auto_turn_interval_is_validated_saved_and_restored_on_reopen`
exercises the settings row, 1..86,400 validation, persistence, reopening the
book, sleep/wake pause, and return to the last book. The `AutoTurn` unit tests
also cover activity resetting the deadline, paused timer behavior, disabling,
and a late tick producing exactly one turn. Verified with `cargo test --locked`: 894 library tests passed, with 66 ignored; the Kindle binary test (1), EPUB package tests (14), and layout probes (4) also passed. The test captures the simulator screenshot above. The focused functional test passed independently as well.

## Remaining parity work

- Let the user choose KOReader's signed fractional scroll distance for continuous reading, including direction and adjustment step.
- Compare USB handoff and actual sleep/resume timing on the Kindle. This Mac simulator check does not validate physical panel refresh or deep-suspend timing.
- Keep on-device acceptance pending while the Kindle is disconnected.
