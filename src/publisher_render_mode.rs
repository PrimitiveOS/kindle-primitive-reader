//! Compose the horizontal Parley policies measured against KOReader's four modes.
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherRenderMode {
    /// Reader’s established layout when no publisher mode is selected.
    #[default]
    Reader,
    Legacy,
    Flat,
    Book,
    Web,
}

impl PublisherRenderMode {
    #[cfg(feature = "kindle")]
    pub(crate) const CHOICES: [Self; 5] = [
        Self::Reader,
        Self::Legacy,
        Self::Flat,
        Self::Book,
        Self::Web,
    ];

    #[cfg(feature = "kindle")]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Reader => "Reader",
            Self::Legacy => "Legacy",
            Self::Flat => "Flat",
            Self::Book => "Book",
            Self::Web => "Web",
        }
    }

    #[cfg(feature = "kindle")]
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Reader => "Reader’s established layout",
            Self::Legacy => "Separate margins and empty breaks",
            Self::Flat => "Joined margins and inline boxes",
            Self::Book => "Float wrapping and text-sized boxes",
            Self::Web => "Publisher sizes and signed margins",
        }
    }

    /// Applies the selected layout policy to preview or reflow settings.
    pub fn apply(self, settings: Settings) -> Settings {
        let active = self != Self::Reader;
        let enhanced = active && self != Self::Legacy;
        let web = self == Self::Web;
        Settings {
            publisher_render_mode: active.then_some(self),
            block_margins: match self {
                Self::Reader => BlockMarginMode::Ignore,
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
                Self::Reader | Self::Legacy => InlineBlockMode::Ignore,
                Self::Flat => InlineBlockMode::Flat,
                Self::Book | Self::Web => InlineBlockMode::BoxSizing,
            },
            fit_content: match self {
                Self::Reader | Self::Legacy | Self::Flat => FitContentMode::Ignore,
                Self::Book => FitContentMode::Book,
                Self::Web => FitContentMode::Web,
            },
            publisher_horizontal_margins: active,
            publisher_signed_horizontal_margins: web,
            publisher_block_height: web,
            float_mode: match self {
                Self::Reader | Self::Legacy | Self::Flat => FloatMode::Ignore,
                Self::Book => FloatMode::ContentHeight,
                Self::Web => FloatMode::PublisherHeight,
            },
            empty_break_mode: match self {
                Self::Reader => EmptyBreakMode::Current,
                Self::Legacy => EmptyBreakMode::PreserveStructure,
                Self::Flat | Self::Book => EmptyBreakMode::Coalesce,
                Self::Web => EmptyBreakMode::PreserveSized,
            },
            empty_break_geometry: active,
            relax_oversized_publisher_keeps: active,
            prefer_publisher_opening_breaks: enhanced,
            ..settings
        }
    }
}

#[cfg(test)]
mod tests;
