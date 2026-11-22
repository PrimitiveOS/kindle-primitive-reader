use super::*;

impl Reader {
    pub(in crate::app) fn poll_read_alarm(&mut self, now: Instant, resumed: bool) -> Change {
        let change = self.read_alarm.tick(now, resumed);
        self.expand_alarm_alert();
        change
    }

    pub(super) fn expand_alarm_alert(&mut self) {
        if self.read_alarm.alert.as_ref().is_some_and(|a| a.expand) {
            let mut alert = self.read_alarm.alert.take().unwrap();
            alert.expand = false;
            if alert.text.contains('%') {
                alert.text = self.alarm_message(&alert.text, SystemTime::now());
            }
            self.read_alarm.alert = Some(alert);
        }
    }

    fn alarm_message(&self, message: &str, wall: SystemTime) -> String {
        use std::collections::HashMap;
        let mut values = HashMap::new();
        let mut chars = message.chars().peekable();
        let mut out = String::new();
        let local = local_clock(wall).map(|(_, civil)| civil);
        let entry = self
            .active
            .as_ref()
            .map(|a| &a.entry)
            .or_else(|| self.fixed.as_ref().map(|f| &f.entry));
        while let Some(ch) = chars.next() {
            if ch != '%' || !chars.peek().is_some_and(char::is_ascii_alphabetic) {
                out.push(ch);
                continue;
            }
            let token = chars.next().unwrap();
            let value = values.entry(token).or_insert_with(|| {
                let value = match token {
                    'T' => self
                        .active
                        .as_ref()
                        .map(|a| a.reading.probe.title.clone())
                        .or_else(|| entry.map(|e| e.title.clone())),
                    'A' => self
                        .active
                        .as_ref()
                        .and_then(|a| a.reading.probe.author.clone())
                        .or_else(|| entry.and_then(|e| e.author.clone())),
                    'S' => self
                        .active
                        .as_ref()
                        .and_then(|a| a.reading.probe.first_series.as_ref())
                        .map(|s| match &s.position {
                            Some(index) => format!("{} #{index}", s.name),
                            None => s.name.clone(),
                        }),
                    'F' => entry
                        .and_then(|e| self.library.absolute_path(&e.path).ok())
                        .map(|p| p.to_string_lossy().into_owned()),
                    'f' => entry
                        .and_then(|e| e.path.file_name())
                        .map(|p| p.to_string_lossy().into_owned()),
                    'r' => Some(" · ".into()),
                    'C' | 'P' | 'p' | 'l' | 'H' | 'h' => self.alarm_progress(token),
                    't' | 'c' => {
                        let last = token == 't';
                        if let Some(f) = &self.fixed {
                            let i = if last {
                                f.reading.count().saturating_sub(1)
                            } else {
                                f.reading.page()
                            };
                            Some(
                                f.reading
                                    .page_label(i)
                                    .unwrap_or_else(|| (i + 1).to_string()),
                            )
                        } else {
                            self.active.as_ref().and_then(|a| {
                                if a.page_labels() > 0 {
                                    let row = if last {
                                        Some(a.page_labels() - 1)
                                    } else {
                                        self.current_publisher_page()
                                    };
                                    return row
                                        .and_then(|r| a.page_label(r))
                                        .map(|s| s.into_owned());
                                }
                                (a.reading.probe.chapter_count() == 1)
                                    .then(|| {
                                        if last {
                                            a.reading.probe.page_count()
                                        } else {
                                            a.reading.last_visible_page() + 1
                                        }
                                    })
                                    .map(|n| n.to_string())
                            })
                        }
                    }
                    'b' => primitive_device::battery().map(|n| n.to_string()),
                    'B' => primitive_device::battery().map(|percent| {
                        if primitive_device::charging() {
                            "⚡".into()
                        } else {
                            match percent {
                                0..=20 => "▁",
                                21..=40 => "▃",
                                41..=60 => "▄",
                                61..=80 => "▆",
                                _ => "█",
                            }
                            .into()
                        }
                    }),
                    'D' | 'd' | 'm' | 'M' => local.map(|dt| match token {
                        'D' => format!(
                            "{:04}-{:02}-{:02}",
                            dt.tm_year + 1900,
                            dt.tm_mon + 1,
                            dt.tm_mday
                        ),
                        'd' => format!("{:02}-{:02}", dt.tm_mon + 1, dt.tm_mday),
                        'm' => format!("{:02}:{:02}", dt.tm_hour, dt.tm_min),
                        _ => format!("{:02}-{:02}-{:02}", dt.tm_hour, dt.tm_min, dt.tm_sec),
                    }),
                    _ => None,
                };
                value.unwrap_or_else(|| "N/A".into())
            });
            out.extend(value.chars().take(512));
            if out.len() > 8192 {
                break;
            }
        }
        out
    }
}
