//! HTML list ordinals and publisher marker styles. Generated labels have no source text offsets.
use roxmltree::{Node, NodeId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    LeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MarkerPosition {
    #[default]
    Outside,
    Inside,
}

impl MarkerKind {
    pub(crate) fn css(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "disc" => Self::Disc,
            "circle" => Self::Circle,
            "square" => Self::Square,
            "decimal" => Self::Decimal,
            "decimal-leading-zero" => Self::LeadingZero,
            "lower-alpha" | "lower-latin" => Self::LowerAlpha,
            "upper-alpha" | "upper-latin" => Self::UpperAlpha,
            "lower-roman" => Self::LowerRoman,
            "upper-roman" => Self::UpperRoman,
            _ => return None,
        })
    }
    pub(crate) fn html(node: Node<'_, '_>) -> Option<Self> {
        if !matches!(node.tag_name().name(), "ol" | "ul" | "li") {
            return None;
        }
        node.attribute("type")
            .and_then(|t| match t {
                "1" => Some(Self::Decimal),
                "a" => Some(Self::LowerAlpha),
                "A" => Some(Self::UpperAlpha),
                "i" => Some(Self::LowerRoman),
                "I" => Some(Self::UpperRoman),
                _ => Self::css(&t.to_ascii_lowercase()),
            })
            .or_else(|| match node.tag_name().name() {
                "ol" => Some(Self::Decimal),
                "ul" => Some(
                    match node
                        .ancestors()
                        .skip(1)
                        .filter(|n| n.has_tag_name("ul") || n.has_tag_name("ol"))
                        .count()
                    {
                        0 => Self::Disc,
                        1 => Self::Circle,
                        _ => Self::Square,
                    },
                ),
                _ => None,
            })
    }
    pub(crate) fn label(self, value: i32) -> Option<String> {
        match self {
            Self::None => None,
            Self::Disc => Some("•".into()),
            Self::Circle => Some("◦".into()),
            Self::Square => Some("▪".into()),
            _ => {
                let mut number = match self {
                    Self::LeadingZero if (-9..=9).contains(&value) => format!(
                        "{}0{}",
                        if value < 0 { "-" } else { "" },
                        value.unsigned_abs()
                    ),
                    Self::LowerAlpha | Self::UpperAlpha if value > 0 => {
                        let mut v = value as u32;
                        let mut chars = Vec::new();
                        while v > 0 {
                            v -= 1;
                            chars.push((b'a' + (v % 26) as u8) as char);
                            v /= 26;
                        }
                        chars.into_iter().rev().collect()
                    }
                    Self::LowerRoman | Self::UpperRoman if (1..4000).contains(&value) => {
                        crate::fixed::roman_page_number(value as u32)?
                    }
                    _ => value.to_string(),
                };
                if matches!(self, Self::UpperAlpha | Self::UpperRoman) {
                    number.make_ascii_uppercase();
                } else if self == Self::LowerRoman {
                    number.make_ascii_lowercase();
                }
                number.push('.');
                Some(number)
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct ListItem {
    pub group: usize,
    pub parent_group: Option<usize>,
    pub element: usize,
    pub depth: usize,
    pub ordinal: i32,
    pub first: bool,
}
/// Raw counter inputs, separate from the historical generated ordinal. Every
/// retained direct element child is included: CSS may give a non-li child a
/// list-item display, and an explicit reset must survive even when it originally
/// equalled the natural ordinal. The existing extraction node cap bounds these
/// records; each retained node belongs to at most one direct-child list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CounterChild {
    pub element: usize,
    pub value: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CounterGroup {
    pub element: usize,
    pub start: Option<i32>,
    pub reversed: bool,
    pub children: Vec<CounterChild>,
}

pub(crate) struct Items {
    by_node: HashMap<NodeId, ListItem>,
    seen: HashSet<usize>,
    pub counters: Vec<CounterGroup>,
}
impl Items {
    pub fn read(body: Node<'_, '_>, elements: &HashMap<NodeId, usize>) -> Self {
        let mut by_node = HashMap::new();
        let mut counters = Vec::new();
        for list in body
            .descendants()
            .filter(|n| n.has_tag_name("ol") || n.has_tag_name("ul"))
        {
            let Some(&group) = elements.get(&list.id()) else {
                continue;
            };
            let parent_group = list
                .ancestors()
                .skip(1)
                .find(|node| node.has_tag_name("ul") || node.has_tag_name("ol"))
                .and_then(|node| elements.get(&node.id()).copied());
            let children = || list.children().filter(|n| n.has_tag_name("li"));
            let reversed = list.has_tag_name("ol") && list.has_attribute("reversed");
            counters.push(CounterGroup {
                element: group,
                start: list.attribute("start").and_then(|v| v.trim().parse().ok()),
                reversed,
                children: list
                    .children()
                    .filter(Node::is_element)
                    .filter_map(|child| {
                        Some(CounterChild {
                            element: *elements.get(&child.id())?,
                            value: child.attribute("value").and_then(|v| v.trim().parse().ok()),
                        })
                    })
                    .collect(),
            });
            let mut value = list
                .attribute("start")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or_else(|| {
                    if reversed {
                        children().count() as i32
                    } else {
                        1
                    }
                });
            let step = if reversed { -1 } else { 1 };
            let depth = list
                .ancestors()
                .filter(|n| n.has_tag_name("ol") || n.has_tag_name("ul"))
                .count();
            for li in children() {
                if let Some(explicit) = li.attribute("value").and_then(|v| v.trim().parse().ok()) {
                    value = explicit
                }
                if let Some(&element) = elements.get(&li.id()) {
                    by_node.insert(
                        li.id(),
                        ListItem {
                            group,
                            parent_group,
                            element,
                            depth,
                            ordinal: value,
                            first: false,
                        },
                    );
                }
                value = value.saturating_add(step);
            }
        }
        Self {
            by_node,
            seen: HashSet::new(),
            counters,
        }
    }
    pub fn block(&mut self, node: Node<'_, '_>) -> Option<ListItem> {
        if self.by_node.is_empty() {
            return None;
        }
        let mut item = node
            .ancestors()
            .find_map(|n| self.by_node.get(&n.id()).copied())?;
        item.first = self.seen.insert(item.element);
        Some(item)
    }
}
/// A literal ordinal remains author text and stays selectable. Suppress only an identical prefix.
pub(crate) fn authored_prefix<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    let end = if let Some(ordinal) = label.strip_suffix('.') {
        text.strip_prefix(ordinal).and_then(|suffix| {
            suffix
                .strip_prefix('.')
                .or_else(|| suffix.strip_prefix(')'))
        })
    } else {
        text.strip_prefix(label)
    };
    let rest = end?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace))
        .then(|| &text[..text.len() - rest.trim_start().len()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    #[test]
    fn nested_list_ordinals_resume_after_inner_lists_and_follow_explicit_resets() {
        let source =
            Source::parse(include_str!("../tests/fixtures/publisher-lists.xhtml")).unwrap();
        let data: Vec<_> = source
            .blocks
            .iter()
            .filter_map(|b| b.list_item.map(|i| (i.ordinal, i.depth, i.first)))
            .collect();
        assert_eq!(
            data,
            [
                (8, 1, true),
                (8, 1, false),
                (12, 1, true),
                (1, 2, true),
                (2, 2, true),
                (13, 1, true),
                (2, 1, true),
                (1, 1, true),
                (1, 1, true)
            ]
        );
        for (kind, value, label) in [
            (MarkerKind::LowerAlpha, 27, "aa."),
            (MarkerKind::UpperAlpha, 52, "AZ."),
            (MarkerKind::UpperRoman, 944, "CMXLIV."),
            (MarkerKind::LowerRoman, 0, "0."),
            (MarkerKind::LeadingZero, -1, "-01."),
            (MarkerKind::LeadingZero, 10, "10."),
        ] {
            assert_eq!(kind.label(value).as_deref(), Some(label));
        }
        assert_eq!(authored_prefix("13. Record the tide.", "13."), Some("13. "));
        assert_eq!(authored_prefix("13) Record the tide.", "13."), Some("13) "));
        assert!(authored_prefix("13.24 metres", "13.").is_none());
        assert!(authored_prefix("I went outside.", "I.").is_none());
    }
}
