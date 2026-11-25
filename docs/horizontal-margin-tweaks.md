# Horizontal publisher-margin tweaks

In **Style tweaks**, choose **Ignore all horizontal margins** to clear
publisher left and right margins throughout a reflowable book. Choose **Ignore
horizontal paragraph margins** to clear only paragraph and list-item margins.
The options are mutually exclusive and use the existing per-book override,
Reader-wide default and anchored reflow workflow. They do not change the
Reader's own page margins.

KOReader offers the same pair of rules and treats them as conflicting choices.
Reader applies them to the CSS subset and layout modes it supports. In modes
where publisher horizontal margins are already ignored, enabling a tweak may
not change the page; arbitrary CREngine box-layout parity remains open.

This implementation candidate has focused CSS-cascade and preference tests,
but the tests, stock runtime comparison, simulator and Kindle verification
have not yet run. [KOReader's margin tweaks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L42-L66) · [Reader preset definitions](../src/user_styles.rs)
