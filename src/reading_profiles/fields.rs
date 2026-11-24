use serde::{Deserialize, Serialize};

use crate::library::{BookPreferences, FixedPreferences};

macro_rules! fields {
    (text { $( $text:ident => $text_label:literal : $( $preference:ident ),+ ; )+ }
     fixed { $( $fixed:ident => $fixed_label:literal : $( $view:ident ),+ ; )+ }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum Field { $( $text, )+ $( $fixed, )+ }

        impl Field {
            pub(crate) const TEXT: &'static [Self] = &[ $( Self::$text, )+ ];
            pub(super) const FIXED: &'static [Self] = &[ $( Self::$fixed, )+ ];

            pub(crate) fn label(self) -> &'static str {
                match self { $( Self::$text => $text_label, )+ $( Self::$fixed => $fixed_label, )+ }
            }

            pub(super) fn merge_text(self, from: &BookPreferences, to: &mut BookPreferences) {
                match self {
                    $( Self::$text => { $( to.$preference = from.$preference.clone(); )+ } )+
                    _ => {}
                }
            }

            pub(super) fn merge_view(self, from: FixedPreferences, to: &mut FixedPreferences) {
                match self {
                    $( Self::$fixed => { $( to.$view = from.$view; )+ } )+
                    _ => {}
                }
            }
        }
    };
}

fields! {
    text {
        Font => "Reading font": font_file;
        FontSize => "Font size": font_size, logical_font_size;
        ContentZoom => "Content zoom": render_dpi;
        Margins => "Margins": margin, margin_sides, margin_left, margin_right, margin_top, margin_bottom, link_vertical_margins;
        LineSpacing => "Line spacing": line_height, line_spacing_percent;
        WordSpacing => "Word spacing": word_spacing, percent_word_spacing;
        WordExpansion => "Word expansion": word_expansion;
        LetterSpacing => "Letter spacing": letter_spacing;
        CjkWidth => "CJK character width": cjk_width_percent;
        ReadingMode => "Page turns or scrolling": scroll;
        FacingPages => "Facing pages": facing_pages;
        Alignment => "Alignment": alignment;
        FontWeight => "Font weight": bold_text, font_weight_delta;
        FontContrast => "Font contrast": text_contrast, font_gamma_index;
        PublisherStyles => "Publisher styles": publisher_styles;
        BaseStyle => "Base stylesheet": base_stylesheet;
        Kerning => "Kerning": font_kerning, font_kerning_mode;
        Hinting => "Font hinting": font_hinting;
        ImageScaling => "Image scaling": smooth_images;
        NightImages => "Night images": invert_night_images;
        PublisherFonts => "Publisher fonts": override_fonts;
        Hyphenation => "Hyphenation": disable_soft_hyphens, automatic_hyphenation;
        PageOverlap => "Page overlap": page_overlap_enabled, page_overlap_lines, page_overlap_style;
    }
    fixed {
        FitAndZoom => "Fit, zoom and reading mode": zoom, width_fit, height_fit, continuous;
        PageRotation => "Page rotation": turns;
        ScanContrast => "Scan contrast": scan_contrast, scan_gamma_tenths;
        ScanWhites => "Scan white level": scan_white_level;
        CropMargin => "Crop margin": crop_margin_hundredths;
        Straighten => "Auto straighten": straighten_limit;
        Dither => "Image dithering": dither;
        ScrollGap => "Page gap": continuous_gap;
        TextSpacing => "Reflow line spacing": fixed_reflow_spacing;
        PanOverlap => "Zoom pan overlap": zoom_overlap_h, zoom_overlap_v;
        PanOrder => "Zoom pan direction": zoom_direction_vertical, zoom_bottom_to_top;
        Spreads => "Facing pages": spread;
        ReadingDirection => "Reading direction": right_to_left;
    }
}
