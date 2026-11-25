use super::{LayoutBlock, PaintBrush, Style};
use parley::{BreakReason, Layout};

// First-line shaping adds one provisional paragraph. Bound the extra work in
// exceptionally large publisher blocks on Kindle.
const MAX_MEASURE_BYTES: usize = 24 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Boundary {
    pub(super) end: usize,
    pub(super) reason: BreakReason,
    pub(super) hyphen: bool,
}

impl Boundary {
    fn read(layout: &Layout<PaintBrush>) -> Option<Self> {
        let line = layout.get(0)?;
        let end = line.text_range().end;
        (end > 0).then(|| Self {
            end,
            reason: line.break_reason(),
            hyphen: line.has_discretionary_hyphen(),
        })
    }

    fn retained(self, layout: &Layout<PaintBrush>) -> bool {
        Self::read(layout).is_some_and(|current| {
            current.end == self.end
                && current.reason == self.reason
                && current.hyphen == self.hyphen
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum Pass {
    Ordinary,
    Measure,
    Styled(Boundary),
}

impl Pass {
    pub(super) fn styled_end(self, text: &str) -> usize {
        match self {
            Self::Ordinary => 0,
            Self::Measure => text.len(),
            Self::Styled(boundary) => boundary.end,
        }
    }

    pub(super) fn exact_break(self) -> Option<Boundary> {
        match self {
            Self::Styled(boundary)
                if matches!(
                    boundary.reason,
                    BreakReason::Regular | BreakReason::Emergency
                ) =>
            {
                Some(boundary)
            }
            _ => None,
        }
    }
}

pub(super) fn layout(
    block: &LayoutBlock<'_>,
    style: &Style,
    measurable_width: bool,
    mut shape: impl FnMut(Pass) -> Layout<PaintBrush>,
) -> Layout<PaintBrush> {
    let supported = block.first_line
        && measurable_width
        && style
            .first_line
            .as_ref()
            .is_some_and(|line| line.host == block.element)
        && !style.preformatted_spaces
        && block.ruby.is_empty()
        && block.text.len() <= MAX_MEASURE_BYTES
        && !block.text.starts_with('\n');
    if supported {
        let measure = shape(Pass::Measure);
        if let Some(boundary) = Boundary::read(&measure)
            && block.text.is_char_boundary(boundary.end)
        {
            drop(measure);
            let styled = shape(Pass::Styled(boundary));
            // The alternate shaping may change the atom at a boundary. Use an
            // ordinary paragraph if the exact styled break cannot be replayed.
            if boundary.retained(&styled) {
                return styled;
            }
        }
    }
    shape(Pass::Ordinary)
}
