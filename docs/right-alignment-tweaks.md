# Right-alignment style tweaks

KOReader's `frontend/ui/data/css_tweaks.lua`, Text direction section, provides
separate options to right-align common text or every element. Reader exposes
both as independent style presets, with the two choices mutually exclusive.
The common-text option covers the body, paragraphs, list items and headings;
the all-elements option also covers every other element. Both use important
user-origin rules to override publisher alignment. The explicit common-element
selectors on the all-elements rule preserve its precedence over Reader's
general paragraph alignment choices. Center headings has the higher priority,
matching KOReader's intended composition of those options.

These options control alignment only. They do not set document direction or
provide complete right-to-left layout. Implementation is pending verification:
no tests, simulations, or Kindle checks were run for this change.
