use primitive_ui::{Canvas, Icon, Input, Rect};

const MINUS: Icon = Icon {
    stroke: &["M5 12 H19"],
    fill: &[],
};
const PLUS: Icon = Icon {
    stroke: &["M5 12 H19", "M12 5 V19"],
    fill: &[],
};

pub(super) struct StepButtons {
    decrease: Rect,
    increase: Rect,
}

impl StepButtons {
    pub(super) const fn new(decrease: Rect, increase: Rect) -> Self {
        Self { decrease, increase }
    }

    pub(super) fn draw(&self, canvas: &mut Canvas) {
        for (rect, icon) in [(self.decrease, MINUS), (self.increase, PLUS)] {
            canvas.button(rect, "", false, false);
            let center = Rect::new(
                rect.x + (rect.w - 40) / 2,
                rect.y + (rect.h - 40) / 2,
                40,
                40,
            );
            let ink = if canvas.is_control_pressed(rect) {
                255
            } else {
                0
            };
            canvas.icon(center, icon, ink);
        }
    }

    pub(super) fn delta(&self, input: Input, held_step: i16) -> Option<i16> {
        let (x, y, step) = match input {
            Input::Tap(x, y) => (x, y, 1),
            Input::LongTap(x, y) => (x, y, held_step),
            _ => return None,
        };
        if self.decrease.contains(x, y) {
            Some(-step)
        } else if self.increase.contains(x, y) {
            Some(step)
        } else {
            None
        }
    }
}
