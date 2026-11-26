// Required Notice: Copyright (C) 2026 Doan Nguyen (https://github.com/DoanNguyenTrong/kindle-primitive)
// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0

//! Bounded numeric tab stops using the same spacing as line and hit geometry.
use alloc::vec::Vec;
use super::spacing::{EffectiveSpacing, Gaps, Justification};
use crate::{Brush, Layout};

const MAX_BYTES: usize = 16 * 1024;
const MAX_TABS: usize = 256;

struct AtomMeasure {
    start: usize,
    end: usize,
    cluster: usize,
    advance: f32,
}

impl<B: Brush> Layout<B> {
    /// Apply numeric tab stops only to a complete, unwrapped,
    /// simple LTR ASCII layout. Returns false without changing spacing when
    /// the source-to-atom mapping or measured font space is ambiguous.
    pub fn set_simple_tab_stops(&mut self, text: &str, width: f32, spaces: f32) -> bool {
        if text.len() > MAX_BYTES || !text.contains('\t') || !width.is_finite() || width <= 0.0
            || !spaces.is_finite() || !(0.0..=32.0).contains(&spaces)
            || !text.bytes().all(|byte| matches!(byte, b'\n' | b'\t' | 0x20..=0x7e))
            || self.is_rtl() || !self.data.inline_boxes.is_empty()
            || !self.data.fast_pair_gaps.is_empty()
            || self.data.space_shrink.iter().any(|value| *value != 0.0)
        {
            return false;
        }
        // The standalone zero was shaped with this single run's selected font,
        // language, features and variations. Missing or unsafe probes fall back.
        let [shaped_run] = self.data.shaped_text.runs() else { return false; };
        let Some(ch) = self.data.shaped_zero_advance else { return false; };
        if !ch.is_finite() || ch <= 0.0 || !shaped_run.font_size.is_finite() { return false; }
        if self.data.runs.first().is_none_or(|run| run.has_mixed_style_atoms) { return false; }
        let mut atoms = Vec::new();
        let mut space: Option<f32> = None;
        for (index, _) in self.data.shaped_text.runs().iter().enumerate() {
            let Some(run) = self.data.runs.get(index) else { return false; };
            let slice = self.data.shaped_text.run_slice(index as u32);
            let spacing = EffectiveSpacing::new(run.spacing, Justification::NONE, &self.data.cluster_spacing);
            for atom in slice.atoms_start() {
                let bytes = slice.text_byte_range(atom.char_range());
                let Some(source) = text.get(bytes.clone()) else { return false; };
                let advance = spacing.atom_advance(&atom);
                if !advance.is_finite() || advance < 0.0 || bytes.start >= bytes.end {
                    return false;
                }
                if source == " " {
                    if advance <= 0.0 || space.is_some_and(|prior| (prior - advance).abs() > 0.01) {
                        return false;
                    }
                    space = Some(advance);
                }
                if (source.contains('\t') && (source != "\t" || atom.shaped_clusters_range().len() != 1))
                    || (source.contains('\n') && (source != "\n" || advance.abs() > 0.01))
                {
                    return false;
                }
                atoms.push(AtomMeasure { start: bytes.start, end: bytes.end,
                    cluster: atom.shaped_clusters_range().start as usize, advance });
                if atoms.len() > MAX_BYTES { return false; }
            }
        }
        let Some(space) = space.or(self.data.shaped_space_advance) else { return false; };
        let step = space * spaces;
        if !step.is_finite() { return false; }
        atoms.sort_unstable_by_key(|atom| atom.start);
        let mut cursor = 0usize;
        let mut x = 0.0f32;
        let mut changes = Vec::new();
        for atom in atoms {
            if atom.start != cursor || atom.end > text.len() { return false; }
            cursor = atom.end;
            let Some(source) = text.get(atom.start..atom.end) else { return false; };
            match source {
                "\n" => x = 0.0,
                "\t" => {
                    let mut advance = if step == 0.0 { 0.0 } else { ((x / step).floor() + 1.0) * step - x };
                    if step > 0.0 && advance < ch * 0.5 { advance += step; }
                    if !advance.is_finite() || advance < 0.0 { return false; }
                    x += advance;
                    changes.push((atom.cluster, advance - atom.advance));
                    if changes.len() > MAX_TABS { return false; }
                }
                _ => x += atom.advance,
            }
            if !x.is_finite() || x > width { return false; }
        }
        if cursor != text.len() || changes.is_empty() { return false; }
        for (cluster, delta) in changes {
            self.data.cluster_spacing.resize(self.data.cluster_spacing.len().max(cluster + 1), Gaps::ZERO);
            self.data.cluster_spacing[cluster].after += delta;
        }
        true
    }
}
