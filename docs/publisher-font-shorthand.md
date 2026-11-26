# Publisher font shorthand

Publisher CSS can use a bounded `font` shorthand such as `italic 1em/1.4 serif` or `bold small-caps 120% "Book Face", serif`. Reader expands a supported declaration into its existing font and line-height properties at the same cascade priority and source position. An omitted style, weight or line height resets to normal; supported font-variant settings and optical sizing also reset to their initial values. The reader's separate kerning control remains in effect.

The supported shorthand accepts normal, italic or oblique style; normal, bold, bolder, lighter or numeric weight; optional small caps; the font sizes and line-height forms already supported by Reader; and up to 16 font families. Line heights include normal, bounded unitless values, percentages and supported relative and absolute lengths; the same limits, computed inheritance and DPI behavior as the standalone property apply. System-font keywords, stretch values, CSS-wide shorthand resets, variable and math functions, and other unsupported combinations invalidate the whole shorthand. Repeated supported shorthands are compacted by importance while retaining other declarations and their order. Strict user CSS does not accept this shorthand. Source text and anchors are unchanged.

This is an EPUB CSS compatibility extension: the pinned CREngine stylesheet parser does not establish KOReader shorthand parity. This implementation has only had source review; rendering, pagination and device behavior remain unverified. [CSS Fonts shorthand](https://www.w3.org/TR/css-fonts-3/#font-prop)

Publisher `font-feature-settings` remains open as a separate CSS compatibility
extension. A future implementation needs to preserve required script shaping,
the reader's kerning policies and interactions with the supported font-variant
properties; the pinned CREngine parser also leaves this property unimplemented.
