# Publisher background colors

Reader accepts named colors, RGB/RGBA functions and three-, four-, six- and eight-digit hexadecimal colors for supported publisher and user `background-color` and single-color `background` declarations. It converts opaque colors to grayscale and keeps transparent colors clear. Existing table-cell and supported block backgrounds use the same paint and geometry paths, and the Pure black and white tweak continues to suppress them.

Only fully opaque or fully transparent colors are supported. Partially transparent colors, `currentColor`, background images and multilayer backgrounds remain unsupported; their declarations are ignored rather than replacing an earlier supported color. The supported layout still does not claim arbitrary CSS background geometry. Source text and saved positions are unchanged. This candidate has had source review only; visual, cascade and device behavior remain unverified.
