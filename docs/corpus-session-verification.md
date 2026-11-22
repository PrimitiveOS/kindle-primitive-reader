# Corpus verification uses the resumed Reader session

The first-frame/resume workflow opens a fresh Reader profile and follows its
normal Continue action. Its optional full-spine diagnostics now retain that
resumed Reader's immutable `session::BookConfig` before dropping the UI.

Each reflow section is opened through `BookConfig::chapter`. This preserves
the actual settings, selected font list, live user-style snapshot, personal
hyphenation rules and facing-page width/fallback logic. The verifier does not
copy the new-book default policy or construct a replacement `Settings::default`.
A missing reflow configuration or one bound to another publication is an error.

This matters because new books now explicitly select Medium percentage word
spacing, while `Settings::default` intentionally retains legacy pixel mode.
Earlier default-Probe spine sweeps remain evidence for that explicitly scoped
baseline; their receipts do not prove the new user-facing spacing policy.

Fixed EPUB pages retain a separate fixed-page decode/fit path and explicit
application viewport. PDF and other fixed formats remain outside the optional
EPUB/MOBI spine pass; their ordinary open/turn/resume workflow is unchanged.

Spine receipts identify `configuration_source` as `resumed_reader` or
`fixed_epub`. Reflow receipts include numeric geometry/spacing values, selected
font **count**, and boolean style/hyphenation flags. They do not serialize font
paths, CSS contents, personal words or document text.

The authored regression opens and resumes an actual Reader with a custom
percentage pair, DPI, selected font, style preset, personal rule and facing
pages. It recreates the actual Probe's frames, source anchors and page mappings
from the snapshot, proves same-size default-Probe pixels differ, verifies
whole-spine page counts differ from the old baseline, and retains the frozen
style snapshot even after a later live change. Additional cases reject missing
or wrong-publication configurations and distinguish unopened/fixed sessions.

Implementation and tests are source-ready. Execution and the next full original
corpus sweep remain pending; this change makes no device or release claim.
