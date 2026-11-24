use super::*;
use primitive_ui::TextStyle;

const SHEET: Rect = Rect::new(86, 410, 1092, 740);
const TITLE: Rect = Rect::new(125, 445, 722, 72);
pub(super) const CLOSE: Rect = Rect::new(933, 437, 205, 78);
const LEFT: i32 = 126;
const RIGHT: i32 = 652;
const TILE_W: i32 = 484;
const TILE_H: i32 = 83;
const fn tile(x: i32, y: i32) -> Rect {
    Rect::new(x, y, TILE_W, TILE_H)
}
pub(super) const PICK_INTERVAL: Rect = tile(LEFT, 565);
pub(super) const PICK_ALARM: Rect = tile(RIGHT, 565);
pub(super) const STOP: Rect = Rect::new(LEFT, 674, 1010, TILE_H);
pub(super) const MODE: Rect = tile(LEFT, 791);
pub(super) const MESSAGE: Rect = tile(RIGHT, 791);
pub(super) const HEADER: Rect = tile(LEFT, 898);
pub(super) const FOOTER: Rect = tile(RIGHT, 898);
pub(super) const AUTO: Rect = Rect::new(LEFT, 1005, 1010, TILE_H);
pub(super) const HR_DOWN: Rect = Rect::new(LEFT, 727, 131, 92);
pub(super) const HR_UP: Rect = Rect::new(461, 727, 149, 92);
pub(super) const MIN_DOWN: Rect = Rect::new(RIGHT, 727, 131, 92);
pub(super) const MIN_UP: Rect = Rect::new(986, 727, 151, 92);
pub(super) const START: Rect = Rect::new(300, 930, 664, 92);
const MESSAGE_TEXT: Rect = Rect::new(LEFT, 544, 1010, 197);
pub(super) const PREVIEW_MESSAGE: Rect = tile(LEFT, 762);
pub(super) const SAVE_MESSAGE: Rect = tile(RIGHT, 762);
const ALERT: Rect = Rect::new(130, 628, 1004, 389);
pub(super) const DISMISS: Rect = Rect::new(179, 890, 427, 83);
pub(super) const REPEAT: Rect = Rect::new(661, 890, 427, 83);
const BANNER: Rect = Rect::new(120, 122, 1024, 140);

impl ReadAlarm {
    pub(in crate::app) fn modal(&self) -> bool {
        self.panel.is_some() || self.alert.as_ref().is_some_and(|a| a.hide.is_none())
    }

    pub(in crate::app) fn draw(&self, canvas: &mut Canvas, plain_reflow: bool) {
        let now = Instant::now();
        if plain_reflow
            && self.options.header
            && let Some(remaining) = self.caption(now)
        {
            let area = Rect::new(WIDTH as i32 - 248, 0, 248, 48);
            canvas.fill(area, 255);
            canvas.text(area, &format!("Timer {remaining}"), CAPTION);
        }
        if let Some(panel) = self.panel {
            canvas.popup(SHEET);
            canvas.text(
                TITLE,
                match panel {
                    Panel::Settings => "Read timer",
                    Panel::Set(Kind::Interval, _) => "Set interval",
                    Panel::Set(Kind::Alarm, _) => "Set alarm",
                    Panel::EditMessage => "Expiry message",
                },
                TextStyle::new(42).bold(),
            );
            canvas.button(
                CLOSE,
                if panel == Panel::Settings {
                    "Close"
                } else {
                    "Back"
                },
                false,
                false,
            );
            match panel {
                Panel::Settings => {
                    canvas.button(PICK_INTERVAL, "Set interval", false, false);
                    canvas.button(PICK_ALARM, "Set alarm", false, false);
                    if let Some(remaining) = self.caption(now) {
                        canvas.button(STOP, &format!("Stop timer · {remaining}"), false, false);
                    } else {
                        canvas.text(STOP, "Timer is off", BODY);
                    }
                    let mode = match self.options.expiry {
                        Expiry::Message => "Message",
                        Expiry::Notification => "Notification",
                        Expiry::Silent => "Nothing",
                    };
                    canvas.button(MODE, &format!("On expiry · {mode}"), false, false);
                    canvas.button(MESSAGE, "Expiry message", false, false);
                    canvas.button(HEADER, "Show in header", self.options.header, false);
                    canvas.button(FOOTER, "Show in footer", self.options.footer, false);
                    canvas.button(
                        AUTO,
                        "Repeat interval automatically",
                        self.options.repeat,
                        false,
                    );
                }
                Panel::Set(kind, minutes) => {
                    canvas.text(Rect::new(LEFT, 588, TILE_W, 64), "Hours", BODY);
                    canvas.text(Rect::new(RIGHT, 588, TILE_W, 64), "Minutes", BODY);
                    for (button, label) in
                        [(HR_DOWN, "-"), (HR_UP, "+"), (MIN_DOWN, "-"), (MIN_UP, "+")]
                    {
                        canvas.button(button, label, false, false);
                    }
                    let field = TextStyle::new(56).bold();
                    canvas.text(
                        Rect::new(304, 728, 137, 94),
                        &format!("{:02}", minutes / 60),
                        field,
                    );
                    canvas.text(
                        Rect::new(837, 728, 137, 94),
                        &format!("{:02}", minutes % 60),
                        field,
                    );
                    canvas.button(
                        START,
                        if kind == Kind::Interval {
                            "Start timer"
                        } else {
                            "Set alarm"
                        },
                        false,
                        false,
                    );
                }
                Panel::EditMessage => {
                    self.editor.draw(canvas, MESSAGE_TEXT, 48, 48, 30);
                    canvas.button(PREVIEW_MESSAGE, "Preview", false, false);
                    canvas.button(SAVE_MESSAGE, "Save", false, false);
                    canvas.keyboard(self.editor.editor.layer, "New line", None);
                }
            }
        }
        if let Some(alert) = &self.alert {
            if alert.hide.is_some() {
                canvas.popup(BANNER);
                canvas.text(
                    Rect::new(154, 143, 956, 102),
                    &alert.text,
                    TextStyle::new(35).lines(2).break_long_words(),
                );
            } else {
                canvas.popup(ALERT);
                canvas.text(
                    Rect::new(183, 654, 885, 185),
                    &alert.text,
                    TextStyle::new(41).lines(3).break_long_words(),
                );
                canvas.button(DISMISS, "Done", false, false);
                if alert.repeat_interval.is_some() {
                    canvas.button(REPEAT, "Repeat", false, false);
                }
            }
        }
    }

    pub(super) fn handle(
        &mut self,
        input: Input,
        now: Instant,
        wall: SystemTime,
    ) -> io::Result<Change> {
        let tap = |rect: Rect| matches!(input, Input::Tap(x,y) if rect.contains(x,y));
        let back =
            matches!(input, Input::PageBack) || matches!(input,Input::Swipe{dx,..} if dx > 80.);
        if let Some(alert) = &self.alert
            && alert.hide.is_none()
        {
            if tap(REPEAT)
                && let Some(interval) = alert.repeat_interval
            {
                self.start(now, interval, Some(interval));
            } else if back || matches!(input, Input::PageForward) || tap(DISMISS) {
                self.alert = None;
            } else {
                return Ok(Change::None);
            }
            return Ok(Change::Partial);
        }
        let Some(panel) = self.panel else {
            return Ok(Change::None);
        };
        if back || tap(CLOSE) {
            self.panel = if panel == Panel::Settings {
                None
            } else {
                Some(Panel::Settings)
            };
            self.update_refresh(now);
            return Ok(Change::Partial);
        }
        match panel {
            Panel::Settings => {
                if tap(PICK_INTERVAL) {
                    let minutes = self
                        .remaining(now)
                        .map(|v| (v.as_secs() / 60).min(u64::from(MAX_INTERVAL)) as u16)
                        .unwrap_or(self.options.minutes);
                    self.panel = Some(Panel::Set(Kind::Interval, minutes));
                } else if tap(PICK_ALARM) {
                    let civil = local_clock(wall)
                        .ok_or_else(|| io::Error::other("The local clock is unavailable"))?;
                    self.panel = Some(Panel::Set(
                        Kind::Alarm,
                        (civil.hour * 60 + civil.minute) as u16,
                    ));
                } else if tap(STOP) && self.running.is_some() {
                    self.stop();
                } else if tap(MESSAGE) {
                    self.editor = Multiline::new(&self.options.message, MESSAGE_LIMIT);
                    self.editor.editor.caret = self.editor.editor.text.len();
                    self.editor.show_caret(48, 4);
                    self.panel = Some(Panel::EditMessage);
                } else {
                    let mut next = self.options.clone();
                    if tap(MODE) {
                        next.expiry = match next.expiry {
                            Expiry::Message => Expiry::Notification,
                            Expiry::Notification => Expiry::Silent,
                            Expiry::Silent => Expiry::Message,
                        };
                    } else if tap(HEADER) {
                        next.header = !next.header;
                    } else if tap(FOOTER) {
                        next.footer = !next.footer;
                    } else if tap(AUTO) {
                        next.repeat = !next.repeat;
                    } else {
                        return Ok(Change::None);
                    }
                    self.options = self.save(next)?;
                }
            }
            Panel::Set(kind, minutes) => {
                let hours = minutes / 60;
                let minute = minutes % 60;
                let max_hour = if kind == Kind::Interval { 17 } else { 23 };
                let hold = matches!(input, Input::LongTap(_, _));
                let hit = |rect: Rect| {
                    tap(rect) || matches!(input, Input::LongTap(x,y) if rect.contains(x,y))
                };
                let hour_step = if hold { 4 } else { 1 };
                let minute_step = if hold { 10 } else { 1 };
                let value = if hit(HR_DOWN) {
                    hours.checked_sub(hour_step).unwrap_or(max_hour) * 60 + minute
                } else if hit(HR_UP) {
                    (if hours + hour_step > max_hour {
                        0
                    } else {
                        hours + hour_step
                    }) * 60
                        + minute
                } else if hit(MIN_DOWN) {
                    hours * 60 + minute.checked_sub(minute_step).unwrap_or(59)
                } else if hit(MIN_UP) {
                    hours * 60
                        + if minute + minute_step > 59 {
                            0
                        } else {
                            minute + minute_step
                        }
                } else if tap(START) {
                    let (delay, text) = match kind {
                        Kind::Interval if minutes == 0 => return Ok(Change::None),
                        Kind::Interval => {
                            let mut next = self.options.clone();
                            next.minutes = minutes;
                            self.options = self.save(next)?;
                            (
                                Duration::from_secs(u64::from(minutes) * 60),
                                format!("Timer started · {:02}:{:02}", minutes / 60, minutes % 60),
                            )
                        }
                        Kind::Alarm => {
                            let alarm = clock_occurrence(wall, minutes).ok_or_else(|| {
                                io::Error::other("The local alarm could not be scheduled")
                            })?;
                            (alarm.delay, alarm.confirmation())
                        }
                    };
                    self.panel = None;
                    self.start(now, delay, (kind == Kind::Interval).then_some(delay));
                    self.alert = Some(Alert {
                        expand: false,
                        text,
                        repeat_interval: None,
                        hide: Some(now + Duration::from_secs(5)),
                    });
                    return Ok(Change::Partial);
                } else {
                    return Ok(Change::None);
                };
                self.panel = Some(Panel::Set(kind, value));
            }
            Panel::EditMessage => {
                let Input::Tap(x, y) = input else {
                    return Ok(Change::None);
                };
                if tap(SAVE_MESSAGE) {
                    let mut next = self.options.clone();
                    next.message = self.editor.editor.string();
                    self.options = self.save(next)?;
                    self.panel = Some(Panel::Settings);
                } else if tap(PREVIEW_MESSAGE) {
                    let text = self.editor.editor.string();
                    self.alert = Some(Alert {
                        text: if text.is_empty() {
                            "Time is up".into()
                        } else {
                            text
                        },
                        expand: true,
                        repeat_interval: None,
                        hide: None,
                    });
                } else if MESSAGE_TEXT.contains(x, y) {
                    self.editor.place(
                        ((y - MESSAGE_TEXT.y as f32) / 48.) as usize,
                        ((x - MESSAGE_TEXT.x as f32) / 20.) as usize,
                        48,
                    );
                } else {
                    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
                    let Some(key) = keyboard_key(&layout, &self.editor.editor, "New line", x, y)
                    else {
                        return Ok(Change::None);
                    };
                    self.editor.press(key);
                    self.editor.show_caret(48, 4);
                }
            }
        }
        self.update_refresh(now);
        Ok(Change::Partial)
    }
}

impl Reader {
    pub(in crate::app) fn open_read_alarm(&mut self) {
        self.show(self.surface_screen());
        self.read_alarm.panel = Some(Panel::Settings);
        self.read_alarm.update_refresh(Instant::now());
    }

    pub(in crate::app) fn read_alarm_input(&mut self, input: Input) -> Change {
        match self
            .read_alarm
            .handle(input, Instant::now(), SystemTime::now())
        {
            Ok(change) => {
                self.expand_alarm_alert();
                change
            }
            Err(error) => self.report(error),
        }
    }
}
