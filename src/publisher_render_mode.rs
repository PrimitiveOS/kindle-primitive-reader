//! Compose the horizontal Parley policies measured against KOReader's four modes.
use crate::{
    BlockMarginMode, BlockWidthMode, EmptyBreakMode, FitContentMode, FloatMode, InlineBlockMode,
    Settings,
};

/// Selects the supported publisher geometry for a horizontal Parley layout.
///
/// Each choice combines policies from the pinned KOReader rendering masks.
/// The available policies handle bounded cases of each mechanism. A choice
/// leaves independent settings, such as publisher styles and page footnotes,
/// under the caller's control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublisherRenderMode {
    Legacy,
    Flat,
    Book,
    Web,
}

impl PublisherRenderMode {
    /// Applies the selected layout policy to preview or reflow settings.
    pub fn apply(self, settings: Settings) -> Settings {
        let enhanced = self != Self::Legacy;
        let web = self == Self::Web;
        Settings {
            block_margins: match self {
                Self::Legacy => BlockMarginMode::Separate,
                Self::Flat | Self::Book => BlockMarginMode::Collapse,
                Self::Web => BlockMarginMode::CollapseNegative,
            },
            preserve_forced_page_margins: enhanced,
            // The observed regular body widths need Web. Book's measurable
            // intrinsic widths are handled separately by FitContentMode.
            block_width: if web {
                BlockWidthMode::PublisherWithConstraints
            } else {
                BlockWidthMode::Ignore
            },
            repair_css_tables: enhanced,
            inline_block_mode: match self {
                Self::Legacy => InlineBlockMode::Ignore,
                Self::Flat => InlineBlockMode::Flat,
                Self::Book | Self::Web => InlineBlockMode::BoxSizing,
            },
            fit_content: match self {
                Self::Legacy | Self::Flat => FitContentMode::Ignore,
                Self::Book => FitContentMode::Book,
                Self::Web => FitContentMode::Web,
            },
            publisher_horizontal_margins: true,
            publisher_signed_horizontal_margins: web,
            publisher_block_height: web,
            float_mode: match self {
                Self::Legacy | Self::Flat => FloatMode::Ignore,
                Self::Book => FloatMode::ContentHeight,
                Self::Web => FloatMode::PublisherHeight,
            },
            empty_break_mode: match self {
                Self::Legacy => EmptyBreakMode::PreserveStructure,
                Self::Flat | Self::Book => EmptyBreakMode::Coalesce,
                Self::Web => EmptyBreakMode::PreserveSized,
            },
            empty_break_geometry: true,
            relax_oversized_publisher_keeps: true,
            prefer_publisher_opening_breaks: enhanced,
            ..settings
        }
    }
}

#[cfg(test)]
mod tests;
