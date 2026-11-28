//! Flow metadata for the canonical-source projection. The strict user stylesheet
//! exposes bounded suppression and paint visibility; other box reclassification
//! remains unsupported.
#![allow(dead_code)] // Remaining display metadata is useful to later provider gates.
use super::Style;
use crate::flow_projection::ElementFlow;

/// Exactly the display value names recognized by the pinned CRE provider.
/// Non-none values are metadata; arbitrary box reclassification is not implemented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Display {
    #[default]
    Inline,
    Ruby,
    RunIn,
    Block,
    LegacyListItem,
    ListItem,
    InlineBlock,
    InlineTable,
    Table,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableColumnGroup,
    TableColumn,
    TableCell,
    TableCaption,
    None,
}
impl Display {
    pub(super) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "inline" => Self::Inline,
            "ruby" => Self::Ruby,
            "run-in" => Self::RunIn,
            "block" => Self::Block,
            "-cr-list-item-final" => Self::LegacyListItem,
            "list-item" => Self::ListItem,
            "inline-block" => Self::InlineBlock,
            "inline-table" => Self::InlineTable,
            "table" => Self::Table,
            "table-row-group" => Self::TableRowGroup,
            "table-header-group" => Self::TableHeaderGroup,
            "table-footer-group" => Self::TableFooterGroup,
            "table-row" => Self::TableRow,
            "table-column-group" => Self::TableColumnGroup,
            "table-column" => Self::TableColumn,
            "table-cell" => Self::TableCell,
            "table-caption" => Self::TableCaption,
            "none" => Self::None,
            _ => return None,
        })
    }
    /// Native shapes already derived from Source's HTML tags. This does not
    /// teach the renderer to turn arbitrary elements into new box types.
    pub(crate) fn native(tag: &str) -> Self {
        match tag {
            "li" => Self::ListItem,
            "ruby" => Self::Ruby,
            "table" => Self::Table,
            "thead" => Self::TableHeaderGroup,
            "tbody" => Self::TableRowGroup,
            "tfoot" => Self::TableFooterGroup,
            "tr" => Self::TableRow,
            "colgroup" => Self::TableColumnGroup,
            "col" => Self::TableColumn,
            "td" | "th" => Self::TableCell,
            "caption" => Self::TableCaption,
            "html" | "ol" | "ul" | "dl" => Self::Block,
            tag if crate::source::is_block_tag(tag) => Self::Block,
            _ => Self::Inline,
        }
    }
    pub(crate) fn is_list_item(self) -> bool {
        matches!(self, Self::ListItem | Self::LegacyListItem)
    }
    pub(crate) fn requires_reclassification(self, tag: &str) -> bool {
        self != Self::None && self != Self::native(tag)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}
impl Visibility {
    pub(super) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            "collapse" => Self::Collapse,
            _ => return None,
        })
    }
}
pub(super) fn resolve(
    parent: ElementFlow,
    display: Display,
    visibility: Visibility,
) -> ElementFlow {
    let suppressed = parent.suppressed || display == Display::None;
    ElementFlow {
        suppressed,
        painted: !suppressed && visibility == Visibility::Visible,
    }
}

/// A retained direct element child of a list container, including non-LI tags.
/// Preserve the original explicit value even when it equalled the old ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListChild {
    pub element: usize,
    pub value: Option<i32>,
}

/// Number generated markers independently of canonical Source::ListItem.ordinal.
/// Hidden-paint items still count; display-suppressed/non-list-item nodes do not.
pub(crate) fn list_ordinals(
    styles: &[Style],
    children: &[ListChild],
    start: Option<i32>,
    reversed: bool,
) -> crate::Result<Vec<Option<i32>>> {
    if children.len() > styles.len() {
        return Err("List metadata exceeds its source elements".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut participating = Vec::with_capacity(children.len());
    for child in children {
        let style = styles
            .get(child.element)
            .ok_or("List metadata refers to a missing element")?;
        if !seen.insert(child.element) {
            return Err("List metadata repeats an element".into());
        }
        participating.push(!style.flow.suppressed && style.display.is_list_item());
    }
    let count = participating.iter().filter(|&&yes| yes).count();
    let mut ordinal = start.unwrap_or(if reversed {
        count.min(i32::MAX as usize) as i32
    } else {
        1
    });
    let step = if reversed { -1 } else { 1 };
    let mut result = Vec::with_capacity(children.len());
    for (child, participates) in children.iter().zip(participating) {
        if !participates {
            result.push(None);
            continue;
        }
        if let Some(value) = child.value {
            ordinal = value;
        }
        result.push(Some(ordinal));
        ordinal = ordinal.saturating_add(step);
    }
    Ok(result)
}
