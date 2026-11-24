//! KOReader's percent-letter expansion for optional local export filenames.
use super::*;
use std::cell::OnceCell;
use std::collections::BTreeMap;

pub(super) const MAX_TEMPLATE_CHARS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Scope {
    Single,
    Multiple,
}

impl Scope {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Single => "Single-book filename",
            Self::Multiple => "Multiple-book filename",
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Self::Single => "Example: %T - %D",
            Self::Multiple => "Example: Reading notes %D",
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) struct Subject<'a> {
    pub(in crate::app) id: Option<u64>,
    pub(in crate::app) title: &'a str,
    pub(in crate::app) author: Option<&'a str>,
    pub(in crate::app) file: Option<&'a Path>,
}

impl<'a> From<&'a BookState> for Subject<'a> {
    fn from(state: &'a BookState) -> Self {
        Self {
            id: Some(state.id),
            title: &state.title,
            author: state.author.as_deref(),
            file: Some(&state.path),
        }
    }
}

pub(in crate::app) struct Context<'a> {
    pub(in crate::app) source: Option<Subject<'a>>,
    pub(in crate::app) bundled: bool,
    pub(in crate::app) started_at: Option<i64>,
    pub(in crate::app) last_opened: Option<&'a Values>,
}

pub(in crate::app) type Values = BTreeMap<char, String>;

pub(super) const GUIDE: &[(&str, &str)] = &[
    ("%T   %A   %S", "Title, author, series"),
    ("%f   %F", "Source filename, full source path"),
    ("%D   %d", "Date (year-month-day), month-day"),
    ("%m   %M", "Local time (hour:minute), hour-minute-second"),
    ("%t   %c", "Total or last page label, current page label"),
    ("%p   %P", "Percent read in the book, in the chapter"),
    ("%C   %l", "Chapter title, pages left in the chapter"),
    (
        "%H   %h",
        "Estimated reading time left in the book, in the chapter",
    ),
    (
        "%b   %B   %r",
        "Battery level, battery symbol, footer separator",
    ),
    (
        "%%",
        "Stays literal on its own. %%T writes % followed by the title.",
    ),
    (
        "%Z",
        "Unknown or unavailable tokens become N-A in filenames.",
    ),
];

pub(super) fn valid_template(text: &str) -> bool {
    text.chars().count() <= MAX_TEMPLATE_CHARS && !text.chars().any(char::is_control)
}

pub(super) fn expand(template: &str, values: &Values) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' && chars.peek().is_some_and(char::is_ascii_alphabetic) {
            let token = chars.next().unwrap();
            result.push_str(values.get(&token).map_or("N/A", String::as_str));
        } else {
            result.push(ch);
        }
    }
    result
}

impl Reader {
    pub(in crate::app) fn filename_values(
        &self,
        template: &str,
        subject: Option<Subject<'_>>,
        started_at: Option<i64>,
    ) -> Values {
        let current = subject
            .and_then(|book| book.id)
            .is_some_and(|id| self.open_book_id() == Some(id));
        let state = subject.and_then(|book| self.library.state(book.id?));
        let file = OnceCell::new();
        let file = || {
            file.get_or_init(|| {
                let path = subject?.file?;
                with_storage(|| self.library.absolute_path(path)).ok()
            })
            .as_deref()
        };
        let clock = OnceCell::new();
        let clock = || {
            clock
                .get_or_init(|| started_at.and_then(footer_custom_text::clock_at))
                .as_ref()
        };
        let battery = OnceCell::new();
        let battery = || battery.get_or_init(primitive_device::battery_status);
        let mut values = Values::new();
        let mut seen = BTreeSet::new();
        for pair in template.as_bytes().windows(2) {
            let [b'%', letter] = pair else { continue };
            if !letter.is_ascii_alphabetic() {
                continue;
            }
            let letter = char::from(*letter);
            if !seen.insert(letter) {
                continue;
            }
            let value = match letter {
                'T' => subject.map(|book| book.title.to_owned()),
                'A' => subject.and_then(|book| book.author).map(str::to_owned),
                'S' => {
                    let series = if current {
                        self.active
                            .as_ref()
                            .and_then(|book| book.reading.probe.first_series.clone())
                    } else {
                        file().and_then(|file| {
                            with_storage(|| {
                                let entry = self.library.resolve(file)?;
                                Ok(self
                                    .library
                                    .metadata(&entry)
                                    .ok()
                                    .and_then(|metadata| metadata.series.into_iter().next()))
                            })
                            .ok()
                            .flatten()
                        })
                    };
                    series.map(|series| match series.position {
                        Some(position) => format!("{} #{position}", series.name),
                        None => series.name,
                    })
                }
                'f' => file()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned()),
                'F' => file().map(|path| path.to_string_lossy().into_owned()),
                't' | 'c' if current => self.export_page_token(letter == 't'),
                'C' | 'P' | 'p' | 'l' | 'H' | 'h' if current => self.export_progress_token(letter),
                'p' => state
                    .and_then(|state| state.reading_percent)
                    .map(|n| n.to_string()),
                'r' if self.open_book_id().is_some() => {
                    Some(self.controls.footer_separator.text().to_owned())
                }
                'b' => battery().percent.map(|percent| percent.to_string()),
                'B' => battery().percent.map(|percent| {
                    let status = battery();
                    reading_footer::battery_symbol(percent, status.charging, status.fully_charged)
                }),
                'D' => clock().map(|clock| clock.date.clone()),
                'd' => clock().map(|clock| clock.short_date.clone()),
                'm' => clock().map(|clock| clock.time.clone()),
                'M' => clock().map(|clock| clock.time_with_seconds.clone()),
                _ => None,
            };
            if let Some(value) = value {
                values.insert(letter, value);
            }
        }
        values
    }

    fn export_page_token(&self, total: bool) -> Option<String> {
        if let Some(book) = &self.fixed {
            let page = if total {
                book.reading.count().checked_sub(1)?
            } else {
                book.reading.page()
            };
            return Some(
                book.reading
                    .page_label(page)
                    .unwrap_or_else(|| (page + 1).to_string()),
            );
        }
        let book = self.active.as_ref()?;
        if book.page_labels() > 0 {
            let row = if total {
                book.page_labels() - 1
            } else {
                self.current_publisher_page()?
            };
            return book.page_label(row).map(|label| label.into_owned());
        }
        let (current, pages) = book.reading.whole_book_screen_pages()?;
        Some(if total { pages } else { current }.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_filename_tokens_follow_ascii_lua_matching_and_expand_once() {
        let values = Values::from([('T', "Parts %A".into()), ('D', "2024-02-29".into())]);
        assert_eq!(
            expand("%D %% %%T %Z %1 %é %T %", &values),
            "2024-02-29 %% %Parts %A N/A %1 %é Parts %A %",
        );
    }

    #[test]
    fn export_filename_tokens_use_active_fixed_pages_and_saved_progress_without_guessing() {
        let fixture = crate::app::tests::Fixture::new();
        let pdf = fixture.root.join("Field chart.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&pdf).unwrap();
        app.jump_fixed(1, None).unwrap();
        let id = app.open_book_id().unwrap();
        let total = app.fixed.as_ref().unwrap().reading.count();
        let state = app.library.state(id).unwrap();
        let values = app.filename_values("%c %t %l %f", Some(state.into()), None);
        assert_eq!(values.get(&'c').map(String::as_str), Some("2"));
        assert_eq!(values[&'t'], total.to_string());
        assert_eq!(values[&'f'], "Field chart.pdf");
        assert!(values[&'l'].parse::<usize>().is_ok());

        app.open_path(&fixture.book).unwrap();
        app.library.state_mut(id).unwrap().reading_percent = Some(54);
        let saved = app.library.state(id).unwrap();
        let values = app.filename_values("%p %c %t", Some(saved.into()), None);
        assert_eq!(expand("%p %c %t", &values), "54 N/A N/A");
    }
}
