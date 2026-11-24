//! Optical margin percentages for the Parley reading provider.
use parley::MarginEdge;

pub(crate) fn edge_rule(
    char: char,
    edge: MarginEdge,
    rtl: bool,
    discretionary: bool,
) -> (u8, bool) {
    if discretionary {
        return (70, false);
    }
    let at_line_end = (edge == MarginEdge::Right) != rtl;
    let percent = match char {
        '\'' | ',' | '-' | '.' | '\x60' | '،' | '۔' | '‐' | '‘' | '’' | '‚' | '‛' => 70,
        '"' | ':' | ';' | '؛' | '“' | '”' | '„' | '‟' => 50,
        '–' => 30,
        '‹' | '›' => {
            if at_line_end {
                40
            } else {
                35
            }
        }
        '«' | '»' => {
            if at_line_end {
                20
            } else {
                15
            }
        }
        '!' | '?' | '¡' | '¿' | '؟' | '—' | '…' => 20,
        '(' | ')' | '[' | ']' | '{' | '}' => 5,
        _ => 0,
    };
    if percent != 0 {
        return (percent, false);
    }
    let sloped = match edge {
        MarginEdge::Left => "AJTVWXYvwxy",
        MarginEdge::Right => "AFKLTVWXYkrtvwxy",
    };
    (if sloped.contains(char) { 5 } else { 0 }, true)
}
