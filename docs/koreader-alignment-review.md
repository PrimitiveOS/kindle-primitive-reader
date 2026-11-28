# Reader ↔ KOReader alignment review (pre-implementation)

Review date: 2026-11-27. Reference: KOReader v2026.07.1, commit `9192014d8bd82a91dc1012473be0f238dedfdb54`, read from a shallow clone of the upstream repository. Reader state: working tree on `7885d35b`, with uncommitted changes.

This review checks the reading surfaces against KOReader's source. It does not measure rendering on a Kindle, and it does not replace the parity checklist in [reading-parity-plan.md](reading-parity-plan.md).

## 1. Blocking: the working tree has not been built

- `git status` shows 139 modified and 15 untracked files. `src/` alone has 105 modified files, with about 13.8k insertions and 1.1k deletions.
- [reading-parity-plan.md](reading-parity-plan.md) says the current batch (Hangul selection, CJK look-ahead, Japanese kana controls, OCR scan reflow, long-chapter windowing, CSS hide/show, double-tap, multi-touch, multiswipe) "has not been compiled or behaviorally tested".
- **Correction (2026-11-27):** the first version of this review said no Rust toolchain was installed. That was wrong. The toolchain is at `~/.rustup/toolchains/stable-aarch64-apple-darwin/bin` (cargo 1.100.0) and is not on `PATH` in the default shell. [ENVIRONMENT.md](../../docs/handoff/ENVIRONMENT.md) gives the `PATH` setup and says to build with `--offline`. No build or test was run in the review pass.
- Disk: about 22 GiB free on the root volume. Keep that in mind before a full `target/` build.

**Recommendation:** before adding features, run `cargo check --offline` and the unit tests for the default `kindle` feature and for the desktop example on the current tree, with the toolchain on `PATH`. Fix anything red, then commit the working tree in reviewable groups. Each new feature added on top of unbuilt code adds to the unverified stack. Do not merge to `main` or tag without the owner's go-ahead ([PIPELINE.md](../../docs/handoff/PIPELINE.md) "What this handoff approves").

## 2. Verified alignments

These were checked against both codebases, not only against the docs.

| Surface | KOReader (pinned) | Reader | Result |
| --- | --- | --- | --- |
| Page-turn tap zones | `defaults.lua` L64–65: backward x 0–1/4, forward x 1/4–1 | `src/app/page_turn_settings.rs:49` (`tap`), default `backward: 25` at :106 | Match |
| Swipe turns | `readerpaging.lua` L380–397: west = next, east = previous, reversed for inverted reading order; menus open only from the menu zones (`readermenu.lua` L126–167) | `page_turn_settings.rs` `swipe_turn`: west = next, east = previous, reversed for RTL, horizontal only with a slope guard (L250) | Match |
| Top menu strip | `defaults.lua` L59–60: 1/8 height, 1/5 in middle half | `src/app/reading_bars.rs:43` `menu_edge_height` (1/8, 1/5 for x in 1/4–3/4) | Match |
| Bottom strip | `DTAP_ZONE_MINIBAR` 12/13 | `reading_bars.rs:1164` stops at `height * 12.0 / 13.0` | Match |
| Default font size | `DCREREADER_CONFIG_DEFAULT_FONT_SIZE = 22` (`defaults.lua` L103) | `src/logical_font.rs:5` `DEFAULT = 22.0` | Match |
| Default line spacing | MEDIUM = 100% (`creoptions.lua` L380) | `src/lib.rs:238` `DEFAULT_LINE_SPACING_PERCENT = 100` | Match |
| Default word spacing | MEDIUM = {95, 75} (`defaults.lua` L165) | `src/word_spacing.rs:19` `MEDIUM` = 95 / 75 | Match |
| Default view mode | page (`defaults.lua` L33) | `scroll: false` default | Match |
| Default render DPI | 96 (`creoptions.lua` L341) | `src/lib.rs` `render_dpi: 96` | Match |
| Default hyphenation | on | `src/lib.rs:544` `automatic_hyphenation: true` | Match |
| Kerning default | best = 3 (`creoptions.lua` L649) | `FontKerningMode` `#[default] Best` (`src/lib.rs:223`) | Match |
| Long-press default | `ask` first; nine actions (`readerhighlight.lua` L364–374) | `SelectionLongPressAction` `#[default] Ask`, same nine actions (`src/reading_controls.rs:512`) | Match |
| Search case | case-insensitive default (`readersearch.lua` L29) | `find_match_case: false` (`src/app.rs:1585`) | Match |
| Progress track | `disable_progress_bar = false` (`readerfooter.lua`) | `ProgressScope` `#[default] Book` (`src/reading_controls.rs:1332`), so the track is drawn | Match |

Geometry and defaults line up on the surfaces above. The risk now is verification, not these numbers.

## 3. Verified divergences and gaps

Ranked by reading impact.

### 3.1 No on-page bookmark indicator (gap)

KOReader's `ReaderView` paints a dog-ear when the current page is bookmarked (`readerview.lua` L246–248, driven by `readerbookmark.lua` L368–376 and `readerdogear.lua`). The icon is sized from the top and right margins, and it is drawn at the top-right corner. The `dogear_visible` flag is set from the bookmark state.

In the reader, bookmark state is used in only two places:

- `src/app/top_config.rs:89` switches the menu label between "Add bookmark" and "Remove bookmark".
- `src/app/skim.rs:638` uses it for the skim marker.

`draw_page` (`src/session.rs:3004`) takes highlights, selection and notices, but no bookmark state. The bar icon `BOOKMARK` is used only in the footer count, the page overview, the fixed-page book map and the saved-marks list. A reader who has bookmarked a page sees no mark on that page.

**Why it matters:** this is the most visible state cue in KOReader's reading view. It is small to implement and easy to test.

**Scope note:** the [source inventory](koreader-source-inventory.md) lists `readerdogear` under the highlight/bookmark row. That row does not mention a page indicator, so the gap was not visible from the docs.

### 3.2 Default footer text items differ from KOReader (decision needed)

KOReader's `ReaderFooter.default_settings` (`readerfooter.lua` ~L494–512) enables by default: page progress (`Page X of Y`), time, pages left, battery, percentage, book time-to-read and chapter time-to-read, along with the progress bar and TOC markers.

The reader's default is `StatusBar::Chapter` (`src/reading_controls.rs:750`). Its custom mask `FooterItem::DEFAULT_MASK` (`:840`) is `CurrentPage | BookProgress | ChapterPagesLeft`. The clock, battery, percentage, and time-to-read items are off by default.

The parity plan assigns clock and battery *settings* to Launcher, but the footer displays them in KOReader and in this reader. The time-to-read items are reading metrics that the plan keeps in scope. Leaving them off may be intended. The owner should confirm which default is wanted before anything changes.

### 3.3 Font hinting default is Off, KOReader's is Auto (unexplained)

KOReader's reflow default is `font_hinting` = 2, which is "auto" (`creoptions.lua` L630–634). The reader's `Settings::default()` sets `FontHinting::Off` (`src/lib.rs:554`). No reader doc records a reason for the choice; the grep found only settings-profile references.

This may be a deliberate e-ink choice, since hinting has a small effect at Oasis resolution. It should either be documented as an intentional divergence or changed to match.

### 3.4 Conflicts with the umbrella skills (decision needed)

Read after the skills in `doannt-kindle/.agents/skills/` (`primitive-ui`, `kindle-device`, `kindle-eink`, `primitive-app-conventions`) and [system-architecture.md](../../docs/system-architecture.md). Reader already follows the core contracts: `deploy/Reader.sh` has the `# Wake: redraw` header and the `SIGUSR1` trap, `src/` has no framebuffer, input, or `lipc` code, and `SystemTheme` follows the launcher's `dark_mode`.

Two items conflict with the rules:

- **Quick Display panel** (`src/app/quick_display.rs`) shows Front light and Warmth sliders and a reading-orientation choice. The system contract says quick settings belong to the launcher, and an app "never draws its own quick settings and never keeps its own copy of these switches" ([system-architecture.md §3–4.3](../../docs/system-architecture.md); [primitive-ui §4](../../.agents/skills/primitive-ui/SKILL.md)).
- **Front light and Warmth in Reader's own settings** (the `Front light −/+` and `Warmth −/+` strings in `src/app/interface_language.rs`, the `FrontLight` gesture in `src/app/gestures.rs`). The Reader scope doc says Kindle-wide settings are "Explicitly excluded from Reader" ([reading-parity-plan.md](reading-parity-plan.md), scope table). The code and that doc disagree.

Either keep these as a reading-specific exception (and update the scope doc), or remove them from Reader and let the launcher own them. The owner should choose.

### 3.5 Owner-tracked gaps (not re-verified here)

The controls doc marks these as **Open**, and this review did not check them:

- Landscape reflow validation for both turns (code exists, device validation open; see [reading-edition-orientation.md](reading-edition-orientation.md)).
- Kindle acceptance for tap zones, footer appearance, selection timing and refresh behavior.
- Exact CREngine stylesheet fidelity and base-stylesheet behavior.
- Wider Japanese lexical coverage; broader OCR language packs.

## 4. What this review did not verify

- Rendering fidelity: line breaks, hyphenation output, and justification compared with crengine. The reader uses Parley and Blitz, so pagination will differ; a page-by-page comparison on real EPUBs is still needed.
- Test status. No test run was executed in this review pass (see the correction in section 1).
- Version status: the reader repo is at `Cargo.toml` 0.1.50 (tags `v0.1.22` through `v0.1.50` dated 22–24 Nov 2026). The umbrella [STATE.md](../../docs/handoff/STATE.md) and [implementation-plan.md](../../docs/implementation-plan.md) still name 0.1.22 as the latest published Reader. Which release is live in Store was not checked here.
- Device behavior: refresh, input latency, memory on long chapters, resume after a crash.
- The 774-control matrix. The inventory checker `scripts/check-koreader-inventory.py` run against the pinned checkout matched all 324 entries, which confirms the inventory names real upstream identifiers. It does not check behavior.

## 5. Suggested order before implementation

1. Build and test the current tree (section 1). Commit the result in reviewable groups.
2. Owner decisions: footer default set (3.2) and hinting default (3.3). Record either as deliberate with a reason, or change to match KOReader.
3. Add the on-page dog-ear (3.1). Mirror KOReader's behavior: show when the current page is bookmarked, toggle with the bookmark action, and size from the top and right margins. Check KOReader's scroll-mode behavior in `readerdogear.lua` before deciding how continuous scroll should show it; this review did not verify that case.
4. Only then take the next items from the owner's open list.

## 6. Reproduce

- KOReader reference: `git clone --depth 1 --branch v2026.07.1 https://github.com/koreader/koreader.git <dir>`
- Inventory check: `python3 scripts/check-koreader-inventory.py <dir>`
- Line references above are to the pinned checkout and to the reader at `7885d35b` with the uncommitted changes listed in section 1.
