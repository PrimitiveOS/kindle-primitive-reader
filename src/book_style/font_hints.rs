//! HTML font and body presentation hints, mapped as the pinned user-agent sheet
//! maps them: `font[color]`, `font[face]`, `font[size]`, and `body[bgcolor]` and
//! `body[text]`. Hints sit below author and user rules, and a user tweak can
//! switch them off.
use super::{
    CascadeOrigin, Declaration, Family, Parser, Property, Size, Winners, choose, font_family,
};
use roxmltree::Node;

pub(crate) struct FontHints(Box<[Declaration]>);

/// Maps a legacy `font size` value (1 to 7, optionally relative with a sign) to a
/// root-relative scale. The values match the pinned engine's named sizes, which
/// truncate `256 * n / d` to whole 256ths, and the unsigned wrap of `3 - n`.
fn legacy_size(value: &str) -> Option<f32> {
    let value = value.trim_start();
    let (sign, digits) = match value.as_bytes().first() {
        Some(b'+') => (1i8, &value[1..]),
        Some(b'-') => (-1i8, &value[1..]),
        _ => (0, value),
    };
    let digits: String = digits.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let number: u64 = digits.parse().unwrap_or(u64::MAX);
    let num = match sign {
        1 => number.saturating_add(3),
        // The pinned engine computes `3 - num` as an unsigned value, so anything above 3 wraps
        // to a large number and clamps to 7.
        -1 if number > 3 => 7,
        -1 => 3 - number,
        _ => number,
    }
    .clamp(1, 7);
    Some(match num {
        1 => 192.0 / 256.0,
        2 => 227.0 / 256.0,
        3 => 1.0,
        4 => 307.0 / 256.0,
        5 => 384.0 / 256.0,
        6 => 2.0,
        _ => 3.0,
    })
}

fn own_color(node: Node<'_, '_>, name: &str) -> Option<u8> {
    let value = node.attribute(name).filter(|v| v.len() <= 128)?;
    Parser::new(value)
        .parse_entirely(super::background)
        .ok()
        .flatten()
}

impl FontHints {
    pub(crate) fn read(node: Node<'_, '_>) -> Option<Self> {
        let mut declarations = Vec::new();
        let mut set = |property| {
            declarations.push(Declaration {
                property,
                important: false,
            })
        };
        match node.tag_name().name() {
            "font" => {
                if let Some(color) = own_color(node, "color") {
                    set(Property::TextColor(Some(color)));
                }
                if let Some(face) = node.attribute("face").filter(|v| v.len() <= 256) {
                    let families: Option<Vec<Family>> = Parser::new(face)
                        .parse_entirely(|input| input.parse_comma_separated(font_family))
                        .ok();
                    if let Some(families) = families.filter(|families| !families.is_empty()) {
                        set(Property::Family(families));
                    }
                }
                if let Some(scale) = node.attribute("size").and_then(legacy_size) {
                    set(Property::Size(Size::Root(scale)));
                }
            }
            "body" => {
                if let Some(color) = own_color(node, "bgcolor") {
                    set(Property::Background(Some(color)));
                }
                if let Some(color) = own_color(node, "text") {
                    set(Property::TextColor(Some(color)));
                }
            }
            _ => return None,
        }
        (!declarations.is_empty()).then(|| Self(declarations.into_boxed_slice()))
    }

    pub(super) fn cascade<'a>(&'a self, winners: &mut Winners<'a>) {
        choose(
            winners,
            &self.0,
            false,
            (0, 1, 0),
            CascadeOrigin::UserAgent,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::legacy_size;

    #[test]
    fn legacy_sizes_follow_the_pinned_named_scale_and_clamp() {
        assert_eq!(legacy_size("3"), Some(1.0));
        assert_eq!(legacy_size("1zz"), Some(192.0 / 256.0));
        assert_eq!(legacy_size("7"), Some(3.0));
        assert_eq!(legacy_size("+1"), Some(307.0 / 256.0));
        assert_eq!(legacy_size("-1"), Some(227.0 / 256.0));
        assert_eq!(legacy_size("-5"), Some(3.0));
        assert_eq!(legacy_size("+9"), Some(3.0));
        assert_eq!(legacy_size("big"), None);
        assert_eq!(legacy_size(""), None);
    }
}
