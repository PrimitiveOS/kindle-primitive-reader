# Publisher text color

Horizontal Parley reading layout accepts publisher and user CSS `color` declarations using named colors, hexadecimal colors and RGB/RGBA values whose quantized alpha is fully opaque. It converts them to grayscale for the reading display. The color inherits through supported text runs and generated, first-letter and first-line styles; `currentColor` inherits the foreground. Text decoration follows the same specified ink. An explicit link-ink style takes precedence, while the Pure black and white tweak and the text-contrast setting suppress publisher text ink. Text-contrast changes are applied at paint time.

Unsupported or partially transparent values are ignored, so an earlier supported declaration can still win. More CSS color functions, vertical and Blitz renderers, and non-text shape markers are outside this slice. Supported [borders](publisher-table-borders.md) also resolve `currentColor` from the element's foreground. Source text and saved positions are unchanged. This candidate has had source review only; rendering, selection, contrast and e-ink behavior still need behavioral validation.

Translucent and transparent text color remains open: proper support needs
opacity-aware glyph and decoration blending against the actual background,
while preserving the monochrome and explicit link-ink overrides.
