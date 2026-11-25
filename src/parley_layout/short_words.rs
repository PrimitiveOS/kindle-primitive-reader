//! Source-preserving subset of CREngine's language-specific short-word rules.

use crate::book_style::Style;
use crate::flow_view::LayoutBlock;
use parley::{LineBreak, WordBreak};

const MAX_BOUNDARIES: usize = 4096;

/// Identify soft wraps after a one-letter word and one ordinary space. Keep
/// the source text intact and skip mixed-language or explicitly tailored runs.
pub(super) fn suppressed(block: &LayoutBlock<'_>, styles: &[Style]) -> Vec<usize> {
    let mut result = Vec::new();
    let mut previous = None;
    for (at, ch) in block.text.char_indices() {
        let start = at + ch.len_utf8();
        if previous == Some(' ') && block.text.as_bytes().get(start) == Some(&b' ') {
            let boundary = start + 1;
            if let Some(next) = block.text.get(boundary..).and_then(|s| s.chars().next())
                && next.is_alphabetic()
            {
                let run_at = |offset: usize| {
                    let runs = &*block.styled_text;
                    let index = runs.partition_point(|run| run.bytes.end <= offset);
                    runs.get(index).filter(|run| run.bytes.contains(&offset))
                };
                if let (Some(letter), Some(space), Some(following)) =
                    (run_at(at), run_at(start), run_at(boundary))
                    && letter.language == space.language
                    && letter.language == following.language
                    && [letter, space, following].iter().all(|run| {
                        styles.get(run.element).is_some_and(|style| {
                            !style.inline_nowrap
                                && !style.preformatted_spaces
                                && style.word_break != WordBreak::BreakAll
                                && style.line_break != Some(LineBreak::Anywhere)
                        })
                    })
                    && letter.language.is_some_and(|language| {
                        let primary = crate::typography_language::canonical_dictionary_language(
                            language.language(),
                        );
                        match primary {
                            "pl" if matches!(language.script(), None | Some("Latn")) => matches!(
                                ch,
                                'a' | 'i'
                                    | 'o'
                                    | 'u'
                                    | 'w'
                                    | 'z'
                                    | 'A'
                                    | 'I'
                                    | 'O'
                                    | 'U'
                                    | 'W'
                                    | 'Z'
                            ),
                            "cs" | "sk" if matches!(language.script(), None | Some("Latn")) => {
                                matches!(
                                    ch,
                                    'A' | 'I'
                                        | 'K'
                                        | 'O'
                                        | 'S'
                                        | 'U'
                                        | 'V'
                                        | 'Z'
                                        | 'i'
                                        | 'k'
                                        | 'o'
                                        | 's'
                                        | 'u'
                                        | 'v'
                                        | 'z'
                                )
                            }
                            "ru" if matches!(language.script(), None | Some("Cyrl")) => matches!(
                                ch,
                                'А' | 'В'
                                    | 'И'
                                    | 'К'
                                    | 'О'
                                    | 'С'
                                    | 'У'
                                    | 'а'
                                    | 'в'
                                    | 'и'
                                    | 'к'
                                    | 'о'
                                    | 'с'
                                    | 'у'
                            ),
                            _ => false,
                        }
                    })
                {
                    result.push(boundary);
                    if result.len() > MAX_BOUNDARIES {
                        return Vec::new();
                    }
                }
            }
        }
        previous = Some(ch);
    }
    result
}
