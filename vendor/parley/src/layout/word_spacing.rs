// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Optional reader space percentages; glyph outlines and source ranges stay intact.
use super::spacing::Gaps;
use crate::{Brush, Layout};
use alloc::vec::Vec;
use core::ops::Range;

impl<B: Brush> Layout<B> {
    /// Scale ordinary spaces and NBSPs after font selection, then permit bounded
    /// shrinking during line fitting. `preserved` contains source ranges whose
    /// spaces are preformatted; they receive neither operation. Call before line
    /// breaking. This is separate from CSS's additive WordSpacing property.
    pub fn set_word_spacing_percent(
        &mut self,
        text: &str,
        scaling: u16,
        minimum: u8,
        preserved: &[Range<usize>],
    ) {
        if !(10..=500).contains(&scaling)
            || !(25..=100).contains(&minimum)
            || (scaling == 100 && minimum == 100)
        {
            return;
        }
        let mut locked = Vec::new();
        let mut offset = 0;
        for paragraph in text.split_inclusive('\n') {
            let length = locked_prefix(paragraph);
            if length > 0 {
                locked.push(offset..offset + length);
            }
            offset += paragraph.len();
        }
        let mut additions = Vec::new();
        for (index, _) in self.data.shaped_text.runs().iter().enumerate() {
            let slice = self.data.shaped_text.run_slice(index as u32);
            for atom in slice.atoms_start() {
                let bytes = slice.text_byte_range(atom.char_range());
                let Some(source) = text.get(bytes.clone()) else {
                    continue;
                };
                if !matches!(source, " " | "\u{a0}") || atom.advance() <= 0.0 {
                    continue;
                }
                let at = preserved.partition_point(|range| range.end <= bytes.start);
                if preserved
                    .get(at)
                    .is_some_and(|range| range.start <= bytes.start && range.end >= bytes.end)
                {
                    continue;
                }
                let cluster = atom.shaped_clusters_range().start as usize;
                let existing = self
                    .data
                    .cluster_spacing
                    .get(cluster)
                    .copied()
                    .unwrap_or(Gaps::ZERO);
                // Scale the effective space including letter and CSS word
                // spacing. Keep Reader's signed tracking extension safe.
                let measured =
                    atom.advance() + existing.total() + self.data.runs[index].spacing.letter
                        + self.data.runs[index].spacing.word;
                let natural = measured.max(0.0);
                let scaled = if scaling == 100 {
                    natural
                } else {
                    natural * f32::from(scaling) / 100.0
                };
                let lock_at = locked.partition_point(|range| range.end <= bytes.start);
                let is_locked = locked
                    .get(lock_at)
                    .is_some_and(|range| range.contains(&bytes.start));
                let eligible = bytes.end < text.len()
                    && !matches!(text[bytes.end..].chars().next(), Some(' ' | '\u{a0}'))
                    && !is_locked;
                let capacity = if eligible {
                    scaled * f32::from(100 - minimum) / 100.0
                } else {
                    0.0
                };
                additions.push((cluster, scaled - measured, capacity));
            }
        }
        for (cluster, delta, capacity) in additions {
            if delta != 0.0 {
                self.data
                    .cluster_spacing
                    .resize(self.data.cluster_spacing.len().max(cluster + 1), Gaps::ZERO);
                self.data.cluster_spacing[cluster].after += delta;
            }
            if capacity > 0.0 {
                self.data
                    .space_shrink
                    .resize(self.data.space_shrink.len().max(cluster + 1), 0.0);
                self.data.space_shrink[cluster] = capacity;
            }
        }
    }
}

// Leading spaces and the gap following a lone opening dialogue/quotation mark
// retain alignment while normal spaces later in the paragraph remain flexible.
fn locked_prefix(text: &str) -> usize {
    let mut end = 0;
    let mut chars = text.char_indices().peekable();
    while let Some(&(at, ch)) = chars.peek() {
        if matches!(ch, ' ' | '\u{a0}') {
            end = at + ch.len_utf8();
            chars.next();
            continue;
        }
        if !opening_mark(ch) {
            break;
        }
        chars.next();
        if chars
            .peek()
            .is_none_or(|&(_, next)| !matches!(next, ' ' | '\u{a0}'))
        {
            break;
        }
        end = at + ch.len_utf8();
    }
    end
}

fn opening_mark(ch: char) -> bool {
    matches!(ch, '\u{2010}'..='\u{2027}' | '\u{2032}'..='\u{205e}' | '*'..='/' | '\u{ab}' | '\u{bb}' | '"' | '\'' | '#')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_locks_leading_and_single_dialogue_marks_without_locking_words() {
        assert_eq!(locked_prefix("\u{a0}  word space"), 4);
        assert_eq!(locked_prefix("— \" word space"), 6);
        assert_eq!(locked_prefix("ordinary word"), 0);
        assert_eq!(locked_prefix("—— word"), 0);
    }
}
