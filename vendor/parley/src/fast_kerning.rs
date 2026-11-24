// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Classic horizontal pair adjustments with FreeType's default pixel fitting.

use alloc::{collections::BTreeMap, vec::Vec};
use parley_engine::ShapedText;
use skrifa::{FontRef, raw::TableProvider};

use crate::FontData;

// The Mac reference has FreeType's optional GPOS kerning disabled. It reads
// the classic, version-zero, horizontal format-zero SFNT pair tables.
const MAX_SUBTABLES: usize = 32;
const MAX_CACHED_FONTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PairGap {
    pub(crate) right: u32,
    left: u32,
    advance: f32,
}

impl PairGap {
    pub(crate) fn on_right(pairs: &[Self], cluster: u32) -> Option<Self> {
        pairs
            .binary_search_by_key(&cluster, |pair| pair.right)
            .ok()
            .map(|index| pairs[index])
    }

    pub(crate) fn advance_on_line(self, line_start: u32) -> f32 {
        if self.left >= line_start {
            self.advance
        } else {
            0.0
        }
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    fonts: BTreeMap<(u64, u32), Option<FontPairs>>,
}

impl Cache {
    fn pairs(&mut self, font: &FontData) -> Option<&FontPairs> {
        let key = (font.data.id(), font.index);
        if self.fonts.len() >= MAX_CACHED_FONTS && !self.fonts.contains_key(&key) {
            self.fonts.clear();
        }
        self.fonts
            .entry(key)
            .or_insert_with(|| FontPairs::from_font(font))
            .as_ref()
    }

    pub(crate) fn collect(&mut self, shaped: &ShapedText, text: &str, output: &mut Vec<PairGap>) {
        for (index, run) in shaped.runs().iter().enumerate() {
            if run.bidi_level.is_rtl() {
                continue;
            }
            let font = &shaped.fonts()[run.font_index].font;
            let Some(pairs) = self.pairs(font) else {
                continue;
            };
            let slice = shaped.run_slice(index as u32);
            let mut previous = None;
            for atom in slice.atoms_start() {
                let range = atom.shaped_clusters_range();
                let bytes = slice.text_byte_range(atom.char_range());
                let source = text.get(bytes).unwrap_or("");
                if source == "\u{ad}" {
                    // A discretionary marker inside a word has no glyph advance.
                    continue;
                }
                let glyph = single_glyph(&atom, source);
                if let Some((left, left_id)) = previous
                    && let Some(right_id) = glyph
                {
                    let adjustment = grid_fit(
                        pairs.design_units(left_id, right_id),
                        run.font_size,
                        pairs.units_per_em,
                    ) as f32;
                    let advance = adjustment.max(-atom.advance());
                    if advance != 0.0 {
                        output.push(PairGap {
                            left,
                            right: range.start,
                            advance,
                        });
                    }
                }
                previous = glyph.map(|id| (range.start, id));
            }
        }
    }
}

fn single_glyph(atom: &parley_engine::Atom<'_>, source: &str) -> Option<u32> {
    let mut characters = source.chars();
    let character = characters.next()?;
    let range = atom.shaped_clusters_range();
    if character.is_control()
        || characters.next().is_some()
        || range.len() != 1
        || atom.advance() <= 0.0
    {
        return None;
    }
    let mut glyphs = atom.slice().shaped_cluster_glyphs(range.start);
    let glyph = glyphs.next()?;
    (glyph.id != 0 && glyphs.next().is_none()).then_some(glyph.id)
}

struct FontPairs {
    units_per_em: u16,
    subtables: Vec<Subtable>,
}

struct Subtable {
    replace: bool,
    pairs: Vec<(u32, i16)>,
}

impl FontPairs {
    fn from_font(data: &FontData) -> Option<Self> {
        let font = FontRef::from_index(data.data.data(), data.index).ok()?;
        let units_per_em = font.head().ok()?.units_per_em();
        let raw = font.table_data(skrifa::Tag::new(b"kern"))?;
        if units_per_em == 0 {
            return None;
        }
        Self::read(raw.as_bytes(), units_per_em)
    }

    fn read(table: &[u8], units_per_em: u16) -> Option<Self> {
        let version = u16_at(table, 0)?;
        if version != 0 {
            return None;
        }
        let count = usize::from(u16_at(table, 2)?).min(MAX_SUBTABLES);
        let mut offset = 4;
        let mut subtables = Vec::new();
        for _ in 0..count {
            let Some(length) = u16_at(table, offset + 2).map(usize::from) else {
                break;
            };
            let Some(coverage) = u16_at(table, offset + 4) else {
                break;
            };
            if length <= 14 {
                break;
            }
            let end = (offset + length).min(table.len());
            if coverage >> 8 == 0
                && coverage & 3 == 1
                && let Some(records) = table.get(offset + 14..end)
            {
                let count = usize::from(u16_at(table, offset + 6)?);
                let pairs = records.chunks_exact(6).take(count).map(|record| {
                    let key = u32::from_be_bytes([record[0], record[1], record[2], record[3]]);
                    let value = i16::from_be_bytes([record[4], record[5]]);
                    (key, value)
                });
                let mut pairs = pairs.collect::<Vec<_>>();
                if !pairs.windows(2).all(|window| window[0].0 < window[1].0) {
                    // Keep the first occurrence in a publisher's unsorted table.
                    pairs.sort_by_key(|pair| pair.0);
                    pairs.dedup_by_key(|pair| pair.0);
                }
                subtables.push(Subtable {
                    replace: coverage & 8 != 0,
                    pairs,
                });
            }
            offset = end;
        }
        (!subtables.is_empty()).then_some(Self {
            units_per_em,
            subtables,
        })
    }

    fn design_units(&self, left: u32, right: u32) -> i32 {
        let key = (left << 16) | right;
        self.subtables.iter().fold(0, |delta, table| {
            let Ok(index) = table.pairs.binary_search_by_key(&key, |pair| pair.0) else {
                return delta;
            };
            let value = i32::from(table.pairs[index].1);
            if table.replace { value } else { delta + value }
        })
    }
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn rounded_ratio(value: i64, denominator: i64) -> i64 {
    value.signum() * ((value.abs() + denominator / 2) / denominator)
}

fn grid_fit(value: i32, size: f32, units_per_em: u16) -> i64 {
    // FreeType receives whole pixel face sizes in CREngine. First apply the
    // 16.16 scaler to reach 26.6 pixels, then perform its small-ppem reduction.
    let ppem = size.round().clamp(1.0, 4096.0) as i64;
    let scale = rounded_ratio((ppem * 64) << 16, i64::from(units_per_em));
    let mut scaled = rounded_ratio(i64::from(value) * scale, 1 << 16);
    if ppem < 25 {
        scaled = rounded_ratio(scaled * ppem, 25);
    }
    (scaled + 32) >> 6
}
