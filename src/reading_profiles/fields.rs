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
                // Profiles written before render-mode capture have no value.
                // Applying one must not silently reset the current book.
                if self == Self::PublisherRenderMode && from.publisher_render_mode.is_none() {
                    return;
                }
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
        PublisherRenderMode => "Render mode": publisher_render_mode;
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
        FitAndZoom => "Fit, zoom and reading mode": zoom, zoom_factor_hundredths, zoom_columns_tenths, zoom_rows_tenths, zoom_fit_axis, manual_zoom, content_fit, width_fit, height_fit, continuous;
        ZoomTargetFit => "Zoom target and fit": zoom, zoom_fit_axis, manual_zoom, content_fit, width_fit, height_fit, continuous;
        ManualZoomFactor => "Manual zoom factor": zoom_factor_hundredths;
        ZoomColumns => "Column zoom count": zoom_columns_tenths;
        ZoomRows => "Row zoom count": zoom_rows_tenths;
        PageRotation => "Page rotation": turns;
        ScanContrast => "Scan contrast": scan_contrast, scan_gamma_tenths;
        ScanWhites => "Scan white level": scan_white_level;
        Dewatermark => "Dewatermark": dewatermark;
        CropMargin => "Crop margin": crop_margin_hundredths;
        Straighten => "Auto straighten": straighten_limit;
        Dither => "Image dithering": dither;
        ScrollGap => "Page gap": continuous_gap;
        TextSpacing => "Reflow line spacing": fixed_reflow_spacing;
        ReflowColumns => "Reflow columns": fixed_reflow_columns;
        ReflowWordGap => "Reflow word gap": fixed_reflow_word_gap;
        ReflowAlignment => "Reflow alignment": fixed_reflow_alignment;
        ReflowDirection => "Reflow writing direction": fixed_reflow_direction, fixed_text_reflow_direction;
        ReflowTextSize => "Reflow text size": fixed_reflow_text_scale;
        PanOverlap => "Zoom pan overlap": zoom_overlap_h, zoom_overlap_v;
        PanOrder => "Zoom pan direction": zoom_direction_vertical, zoom_bottom_to_top;
        Spreads => "Facing pages": spread, epub_spread;
        ReadingDirection => "Reading direction": right_to_left, epub_right_to_left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::FixedContentFit;

    #[test]
    fn fixed_zoom_profile_fields_store_target_and_counts_independently() {
        let source = FixedPreferences {
            zoom_factor_hundredths: Some(175),
            zoom_columns_tenths: Some(25),
            zoom_rows_tenths: Some(15),
            zoom_fit_axis: Some(FixedContentFit::Height),
            manual_zoom: true,
            ..FixedPreferences::default()
        };
        let mut target = FixedPreferences::default();
        Field::ZoomTargetFit.merge_view(source, &mut target);
        Field::ManualZoomFactor.merge_view(source, &mut target);
        Field::ZoomColumns.merge_view(source, &mut target);
        Field::ZoomRows.merge_view(source, &mut target);

        assert_eq!(target.zoom_factor_hundredths, Some(175));
        assert_eq!(target.zoom_columns_tenths, Some(25));
        assert_eq!(target.zoom_rows_tenths, Some(15));
        assert_eq!(target.zoom_fit_axis, Some(FixedContentFit::Height));
        assert!(target.manual_zoom);
    }

    #[test]
    fn legacy_fixed_zoom_mask_expands_to_granular_fields() {
        let decoded = crate::reading_profiles::decode(serde_json::json!([{
            "name": "Legacy zoom",
            "fixed": { "settings": {} },
            "fields": ["fit_and_zoom"]
        }]))
        .unwrap();

        assert_eq!(
            decoded[0].selection(),
            [
                Field::ZoomTargetFit,
                Field::ManualZoomFactor,
                Field::ZoomColumns,
                Field::ZoomRows,
            ]
        );
    }

    #[test]
    fn dewatermark_is_a_fixed_scan_profile_field_for_supported_formats() {
        use crate::reading_profiles::FixedProvider;

        for provider in [FixedProvider::Pdf, FixedProvider::Djvu, FixedProvider::Comic] {
            assert!(provider.fields().any(|field| field == Field::Dewatermark));
        }
        for provider in [FixedProvider::Epub, FixedProvider::Image] {
            assert!(!provider.fields().any(|field| field == Field::Dewatermark));
        }

        let source = FixedPreferences {
            dewatermark: true,
            ..FixedPreferences::default()
        };
        let mut target = FixedPreferences::default();
        Field::Dewatermark.merge_view(source, &mut target);
        assert!(target.dewatermark);
    }
}
