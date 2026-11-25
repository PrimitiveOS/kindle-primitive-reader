// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Context for layout.

use super::FontContext;
use super::context::LayoutContext;
use super::style::{Brush, StyleProperty, TextStyle};

use super::layout::Layout;

use alloc::string::String;
use core::ops::{Bound, Range, RangeBounds};
use parlance::BaseDirection;
use parley_engine::break_overrides::LineBreakOverrideFn;

use crate::inline_box::{InlineBox, LayoutInlineBox};
use crate::resolve::{ResolvedStyle, StyleRun};

/// The reading application's choice for one authored soft-hyphen offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoftHyphenAction {
    /// Add a discretionary opportunity and draw its hyphen if chosen and it fits.
    Draw,
    /// Suppress the opportunity associated with the soft hyphen.
    Suppress,
    /// Keep Parley's ordinary decision for this boundary without drawing a hyphen.
    LeaveBoundary,
}

/// Extra advance in whole pixels for a shaped atom, given its source, natural advance
/// and selected font size. The glyphs are centered within the added space.
pub type ClusterSpacingFn = dyn Fn(&str, f32, f32) -> u16 + Send + Sync;

/// Signed advance for a requested font weight, after font selection.
pub type FontWeightSpacingFn = dyn Fn(&crate::FontData, f32, f32) -> f32 + Send + Sync;

#[derive(Clone, Copy)]
pub(crate) struct BuilderOptions<'a> {
    scale: f32,
    quantize: bool,
    base_direction: BaseDirection,
    bidi_controls: &'a [(usize, char)],
    relaxed_nonbreaking: &'a [usize],
    suppressed_soft_breaks: &'a [usize],
    line_break_override: Option<&'a LineBreakOverrideFn>,
    automatic_hyphens: &'a [usize],
    trusted_soft_hyphen_words: &'a [Range<usize>],
    cluster_spacing: Option<&'a ClusterSpacingFn>,
    fast_kerning: bool,
    font_weight_spacing: Option<&'a FontWeightSpacingFn>,
    soft_hyphen_policy: Option<&'a (dyn Fn(usize) -> SoftHyphenAction + Send + Sync)>,
}

impl BuilderOptions<'_> {
    pub(crate) fn new(scale: f32, quantize: bool) -> Self {
        Self {
            scale,
            quantize,
            base_direction: BaseDirection::Auto,
            bidi_controls: &[],
            relaxed_nonbreaking: &[],
            suppressed_soft_breaks: &[],
            line_break_override: None,
            soft_hyphen_policy: None,
            automatic_hyphens: &[],
            trusted_soft_hyphen_words: &[],
            cluster_spacing: None,
            fast_kerning: false,
            font_weight_spacing: None,
        }
    }
}

/// Builder for constructing a text layout with ranged attributes.
///
/// This builder does not collapse whitespace. For whitespace collapsing, use the [`TreeBuilder`]
#[must_use]
pub struct RangedBuilder<'a, B: Brush> {
    pub(crate) options: BuilderOptions<'a>,
    pub(crate) lcx: &'a mut LayoutContext<B>,
    pub(crate) fcx: &'a mut FontContext,
}

impl<'b, B: Brush> RangedBuilder<'b, B> {
    pub fn push_default<'a>(&mut self, property: impl Into<StyleProperty<'a, B>>) {
        let resolved =
            self.lcx
                .rcx
                .resolve_property(self.fcx, &property.into(), self.options.scale);
        self.lcx.ranged_style_builder.push_default(resolved);
    }

    pub fn push<'a>(
        &mut self,
        property: impl Into<StyleProperty<'a, B>>,
        range: impl RangeBounds<usize>,
    ) {
        let resolved =
            self.lcx
                .rcx
                .resolve_property(self.fcx, &property.into(), self.options.scale);
        self.lcx.ranged_style_builder.push(resolved, range);
    }

    pub fn push_inline_box(&mut self, inline_box: InlineBox) {
        self.lcx.inline_boxes.push(LayoutInlineBox {
            inline_box,
            parent_style_index: 0,
            baseline_offset: 0.,
        });
    }

    /// Sets the paragraph's base direction.
    ///
    /// The default is [`BaseDirection::Auto`], which infers the direction from the text.
    pub fn set_base_direction(&mut self, base_direction: BaseDirection) {
        self.options.base_direction = base_direction;
    }

    /// Add virtual bidi controls without inserting bytes into the layout text.
    pub fn set_bidi_controls(&mut self, controls: &'b [(usize, char)]) {
        self.options.bidi_controls = controls;
    }

    /// Mark source byte boundaries for conservative non-breaking character
    /// relaxation without altering the text or its selection offsets.
    pub fn set_relaxed_nonbreaking(&mut self, boundaries: &'b [usize]) {
        self.options.relaxed_nonbreaking = boundaries;
    }

    /// Suppress selected soft wraps at source UTF-8 boundaries without
    /// changing the text. Mandatory breaks are never suppressed.
    pub fn set_suppressed_soft_breaks(&mut self, boundaries: &'b [usize]) {
        self.options.suppressed_soft_breaks = boundaries;
    }

    /// Set the callback which will be called as a first provider of line breaking decisions.
    ///
    /// See [`LineBreakOverrideFn`] for more details.
    pub fn set_line_break_override(&mut self, overrides: Option<&'b LineBreakOverrideFn>) {
        self.options.line_break_override = overrides;
    }

    /// Configure discretionary soft hyphens in ranged text. The callback receives
    /// the original UTF-8 byte offset of each eligible U+00AD.
    /// Passing `None` retains upstream Parley behavior.
    ///
    /// This PrimitiveOS extension handles standalone soft-hyphen clusters in
    /// the selected face and preserves the source cluster for text selection.
    pub fn set_soft_hyphen_policy(
        &mut self,
        policy: Option<&'b (dyn Fn(usize) -> SoftHyphenAction + Send + Sync)>,
    ) {
        self.options.soft_hyphen_policy = policy;
    }

    /// Set extra space around selected shaped atoms before line breaking. The
    /// source is unchanged, including ligatures, combining marks and variations.
    pub fn set_cluster_spacing(&mut self, policy: Option<&'b ClusterSpacingFn>) {
        self.options.cluster_spacing = policy;
    }

    /// Apply classic horizontal pair adjustments using FreeType's default
    /// pixel fitting. Disable the shaper's 'kern' feature to avoid applying
    /// both OpenType and classic spacing.
    pub fn set_fast_kerning(&mut self, enabled: bool) {
        self.options.fast_kerning = enabled;
    }

    /// Apply font-weight synthesis advances to layout and hit testing.
    pub fn set_font_weight_spacing(&mut self, policy: Option<&'b FontWeightSpacingFn>) {
        self.options.font_weight_spacing = policy;
    }

    /// Set sorted UTF-8 source boundaries where a visible hyphen may be appended
    /// if the line ends there. A break can only follow a complete LTR shaping atom.
    pub fn set_automatic_hyphens(&mut self, boundaries: &'b [usize]) {
        self.options.automatic_hyphens = boundaries;
    }

    /// For these sorted nonoverlapping source words, a fitting authored
    /// hyphen takes precedence over a generated break in the same word.
    pub fn set_trusted_soft_hyphen_words(&mut self, words: &'b [Range<usize>]) {
        self.options.trusted_soft_hyphen_words = words;
    }

    pub fn build_into(self, layout: &mut Layout<B>, text: impl AsRef<str>) {
        // Apply RangedStyleBuilder styles directly to style-table/style-run state.
        self.lcx
            .ranged_style_builder
            .finish(&mut self.lcx.style_table, &mut self.lcx.style_runs);

        // Call generic layout builder method
        build_into_layout(layout, text.as_ref(), self.lcx, self.fcx, self.options);
    }

    pub fn build(self, text: impl AsRef<str>) -> Layout<B> {
        let mut layout = Layout::default();
        self.build_into(&mut layout, text);
        layout
    }
}

/// Builder for constructing a text layout from a style table and
/// indexed style runs.
///
/// This builder does not collapse whitespace. For whitespace collapsing, use the [`TreeBuilder`]
#[must_use]
pub struct StyleRunBuilder<'a, B: Brush> {
    pub(crate) options: BuilderOptions<'a>,
    pub(crate) len: usize,
    pub(crate) lcx: &'a mut LayoutContext<B>,
    pub(crate) fcx: &'a mut FontContext,
    pub(crate) cursor: usize,
}

impl<'b, B: Brush> StyleRunBuilder<'b, B> {
    /// Reserves additional capacity for styles and runs.
    ///
    /// This is an optional optimization for callers that know counts
    /// up front; call it before pushing styles and runs to reduce
    /// reallocations.
    pub fn reserve(&mut self, additional_styles: usize, additional_runs: usize) {
        self.lcx.style_table.reserve(additional_styles);
        self.lcx.style_runs.reserve(additional_runs);
    }

    /// Adds a fully-specified style to the shared style table and
    /// returns its index.
    pub fn push_style<'family, 'settings>(
        &mut self,
        style: TextStyle<'family, 'settings, B>,
    ) -> u16 {
        let resolved = self
            .lcx
            .rcx
            .resolve_entire_style_set(self.fcx, &style, self.options.scale);
        let style_index = self.lcx.style_table.len();
        assert!(style_index <= u16::MAX as usize, "too many styles");
        self.lcx.style_table.push(resolved);
        style_index as u16
    }

    /// Adds a style run referencing an entry from the style table.
    ///
    /// Runs must be contiguous and non-overlapping, and must cover
    /// `0..text.len()` once all runs have been added.
    pub fn push_style_run(&mut self, style_index: u16, range: impl RangeBounds<usize>) {
        let range = resolve_range(range, self.len);
        assert!(
            range.start == self.cursor,
            "StyleRunBuilder expects contiguous non-overlapping runs"
        );
        assert!(
            range.start <= range.end,
            "StyleRunBuilder expects ordered ranges"
        );
        assert!(
            (style_index as usize) < self.lcx.style_table.len(),
            "StyleRunBuilder expects style indices that were previously added via push_style"
        );
        self.lcx.style_runs.push(StyleRun {
            style_index,
            range: range.clone(),
        });
        self.cursor = range.end;
    }

    pub fn push_inline_box(&mut self, inline_box: InlineBox) {
        self.lcx.inline_boxes.push(LayoutInlineBox {
            inline_box,
            parent_style_index: 0,
            baseline_offset: 0.,
        });
    }

    /// Sets the paragraph's base direction.
    ///
    /// The default is [`BaseDirection::Auto`], which infers the direction from the text.
    pub fn set_base_direction(&mut self, base_direction: BaseDirection) {
        self.options.base_direction = base_direction;
    }

    /// Set the callback which will be called as a first provider of line breaking decisions.
    ///
    /// See [`LineBreakOverrideFn`] for more details.
    pub fn set_line_break_override(&mut self, overrides: Option<&'b LineBreakOverrideFn>) {
        self.options.line_break_override = overrides;
    }

    pub fn build_into(self, layout: &mut Layout<B>, text: impl AsRef<str>) {
        assert!(
            self.cursor == self.len,
            "StyleRunBuilder requires runs that cover the full text"
        );
        if self.lcx.style_runs.is_empty() {
            // This is reachable for empty text. In this case, the layout still needs a style and an
            // empty style run, e.g. to size a cursor.
            if self.lcx.style_table.is_empty() {
                let style = self.lcx.rcx.resolve_entire_style_set(
                    self.fcx,
                    &TextStyle::default(),
                    self.options.scale,
                );
                self.lcx.style_table.push(style);
            }
            self.lcx.style_runs.push(StyleRun {
                style_index: 0,
                range: 0..0,
            });
        }
        build_into_layout(layout, text.as_ref(), self.lcx, self.fcx, self.options);
    }

    pub fn build(self, text: impl AsRef<str>) -> Layout<B> {
        let mut layout = Layout::default();
        self.build_into(&mut layout, text);
        layout
    }
}

/// Builder for constructing a text layout with a tree of attributes.
#[must_use]
pub struct TreeBuilder<'a, B: Brush> {
    pub(crate) options: BuilderOptions<'a>,
    pub(crate) lcx: &'a mut LayoutContext<B>,
    pub(crate) fcx: &'a mut FontContext,
}

impl<'b, B: Brush> TreeBuilder<'b, B> {
    pub fn push_style_span(&mut self, style: TextStyle<'_, '_, B>) {
        let resolved = self
            .lcx
            .rcx
            .resolve_entire_style_set(self.fcx, &style, self.options.scale);
        self.lcx.tree_style_builder.push_style_span(resolved);
    }

    pub fn push_style_modification_span<'s, 'iter>(
        &mut self,
        properties: impl IntoIterator<Item = &'iter StyleProperty<'s, B>>,
    ) where
        's: 'iter,
        B: 'iter,
    {
        self.lcx
            .tree_style_builder
            .push_style_modification_span(properties.into_iter().map(|p| {
                self.lcx
                    .rcx
                    .resolve_property(self.fcx, p, self.options.scale)
            }));
    }

    pub fn pop_style_span(&mut self) {
        self.lcx.tree_style_builder.pop_style_span();
    }

    pub fn push_text(&mut self, text: &str) {
        self.lcx.tree_style_builder.push_text(text);
    }

    /// Returns the pushed text so far, after whitespace processing, but excluding any trailing
    /// collapsible whitespace.
    ///
    /// This is a prefix of the text that [`Self::build`] will return. Whether that trailing
    /// collapsible whitespace ends up being kept (as a single space) depends on where it is and on
    /// what follows it. See [`Self::has_pending_whitespace`].
    pub fn text(&self) -> &str {
        self.lcx.tree_style_builder.text()
    }

    /// Returns whether [`Self::text`] is followed by collapsible whitespace.
    ///
    /// That white space is not yet part of [`Self::text`]: depending on where it is and what
    /// follows, it is either removed or kept as a single space.
    pub fn has_pending_whitespace(&self) -> bool {
        self.lcx.tree_style_builder.has_pending_whitespace()
    }

    pub fn push_inline_box(&mut self, mut inline_box: InlineBox) {
        if inline_box.kind.is_in_flow() {
            self.lcx.tree_style_builder.flush_pending_whitespace();
            self.lcx.tree_style_builder.set_last_item_is_inline_box();
        }

        // TODO: arrange type better here to factor out the index
        inline_box.index = self.lcx.tree_style_builder.text().len();
        let parent_style_index = self.lcx.tree_style_builder.resolve_current_style_id();
        self.lcx.inline_boxes.push(LayoutInlineBox {
            inline_box,
            parent_style_index,
            baseline_offset: 0.,
        });
    }

    /// Sets the paragraph's base direction.
    ///
    /// The default is [`BaseDirection::Auto`], which infers the direction from the text.
    pub fn set_base_direction(&mut self, base_direction: BaseDirection) {
        self.options.base_direction = base_direction;
    }

    /// Set the callback which will be called as a first provider of line breaking decisions.
    ///
    /// See [`LineBreakOverrideFn`] for more details.
    pub fn set_line_break_override(&mut self, overrides: Option<&'b LineBreakOverrideFn>) {
        self.options.line_break_override = overrides;
    }

    #[inline]
    pub fn build_into(self, layout: &mut Layout<B>) -> String {
        // Apply TreeStyleBuilder styles to LayoutContext.
        let text = self
            .lcx
            .tree_style_builder
            .finish(&mut self.lcx.style_table, &mut self.lcx.style_runs);

        // Call generic layout builder method
        build_into_layout(layout, &text, self.lcx, self.fcx, self.options);

        text
    }

    #[inline]
    pub fn build(self) -> (Layout<B>, String) {
        let mut layout = Layout::default();
        let text = self.build_into(&mut layout);
        (layout, text)
    }
}

fn build_into_layout<B: Brush>(
    layout: &mut Layout<B>,
    text: &str,
    lcx: &mut LayoutContext<B>,
    fcx: &mut FontContext,
    options: BuilderOptions<'_>,
) {
    if text.is_empty() && lcx.style_runs.is_empty() {
        lcx.style_table.push(ResolvedStyle::default());
        lcx.style_runs.push(StyleRun {
            style_index: 0,
            range: 0..0,
        });
    }
    assert!(
        !lcx.style_runs.is_empty(),
        "at least one style run is required"
    );

    crate::analysis::analyze_text(
        lcx,
        text,
        options.base_direction,
        options.line_break_override,
        options.bidi_controls,
        options.relaxed_nonbreaking,
        options.suppressed_soft_breaks,
    );

    layout.data.clear();
    layout.data.scale = options.scale;
    layout.data.quantize = options.quantize;
    layout.data.base_level = lcx.analysis.paragraph_level();
    layout.data.text_len = text.len();

    // Characters not covered by any style run (e.g. the space substituted for empty text during
    // analysis) use the root style.
    lcx.char_style_indices.clear();
    lcx.char_style_indices
        .resize(lcx.analysis.char_info().len(), 0);
    let mut char_index = 0;
    for style_run in &lcx.style_runs {
        for _ in text[style_run.range.clone()].chars() {
            lcx.char_style_indices[char_index] = style_run.style_index;
            char_index += 1;
        }
    }

    // Copy the visual styles into the layout
    layout
        .data
        .styles
        .extend(lcx.style_table.iter().map(|s| s.as_layout_style()));

    // Sort the inline boxes as subsequent code assumes that they are in text index order.
    // Note: It's important that this is a stable sort to allow users to control the order of contiguous inline boxes
    //
    // TODO: consider dropping the sort and requiring `push_inline_box` callers to push boxes in text index order
    // (as `TreeBuilder` already does).
    lcx.inline_boxes.sort_by_key(|b| b.inline_box.index);

    {
        super::shape::shape_text(
            &lcx.rcx,
            fcx,
            &lcx.style_table,
            &lcx.inline_boxes,
            &lcx.analysis,
            &lcx.char_style_indices,
            &mut lcx.scx,
            text,
            layout,
            &lcx.analysis_data_sources,
        );
    }

    if options.fast_kerning {
        lcx.fast_kerning.collect(
            &layout.data.shaped_text,
            text,
            &mut layout.data.fast_pair_gaps,
        );
    }

    if let Some(policy) = options.cluster_spacing {
        let shaped = &layout.data.shaped_text;
        for (index, run) in shaped.runs().iter().enumerate() {
            let slice = shaped.run_slice(index as u32);
            for atom in slice.atoms_start() {
                if atom.advance() <= 0.0 {
                    continue;
                }
                let bytes = slice.text_byte_range(atom.char_range());
                // The empty-paragraph shaping sentinel has no source range.
                let Some(source) = text.get(bytes) else {
                    continue;
                };
                let extra = policy(source, atom.advance(), run.font_size);
                if extra != 0 {
                    let cluster = atom.shaped_clusters_range().start as usize;
                    if cluster >= layout.data.cluster_spacing.len() {
                        layout
                            .data
                            .cluster_spacing
                            .resize(cluster + 1, crate::layout::Gaps::ZERO);
                    }
                    layout.data.cluster_spacing[cluster] = crate::layout::Gaps {
                        before: f32::from(extra / 2),
                        after: f32::from(extra - extra / 2),
                    };
                }
            }
        }
    }

    if let Some(policy) = options.font_weight_spacing {
        let shaped = &layout.data.shaped_text;
        for (index, run) in shaped.runs().iter().enumerate() {
            let font = &shaped.fonts()[run.font_index].font;
            let requested = layout.data.runs[index].font_attrs.weight.value();
            let extra = policy(font, requested, run.font_size);
            if !extra.is_finite() || extra == 0.0 || extra.abs() > 32.0 {
                continue;
            }
            let slice = shaped.run_slice(index as u32);
            for atom in slice.atoms_start() {
                if atom.advance() <= 0.0 {
                    continue;
                }
                let cluster = atom.shaped_clusters_range().start as usize;
                if cluster >= layout.data.cluster_spacing.len() {
                    layout
                        .data
                        .cluster_spacing
                        .resize(cluster + 1, crate::layout::Gaps::ZERO);
                }
                layout.data.cluster_spacing[cluster].after += extra.max(-atom.advance());
            }
        }
    }

    if let Some(policy) = options.soft_hyphen_policy {
        super::shape::collect_soft_hyphens(text, layout, policy, options.trusted_soft_hyphen_words);
    }

    if !options.automatic_hyphens.is_empty() {
        super::shape::collect_automatic_hyphens(
            layout,
            options.automatic_hyphens,
            options.trusted_soft_hyphen_words,
        );
    }

    // After shaping, so that the metrics of fonts used for shaping can be reused.
    crate::layout::style_metrics::resolve_style_metrics(
        &lcx.rcx,
        fcx,
        &lcx.style_table,
        &layout.data.shaped_text,
        options.quantize,
        &mut layout.data.style_metrics,
    );

    // Move inline boxes into the layout
    layout.data.inline_boxes.clear();
    core::mem::swap(&mut layout.data.inline_boxes, &mut lcx.inline_boxes);
}

fn resolve_range(range: impl RangeBounds<usize>, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Unbounded => 0,
        Bound::Included(n) => *n,
        Bound::Excluded(n) => *n + 1,
    };
    let end = match range.end_bound() {
        Bound::Unbounded => len,
        Bound::Included(n) => *n + 1,
        Bound::Excluded(n) => *n,
    };
    start.min(len)..end.min(len)
}
