//! Supported style controls, book-specific CSS review and multiline editing.
use super::navigation_layout::NavigationLayout;
use super::*;
use crate::user_styles::{
    self, CLASSIC_FOOTNOTES, Data, EPUB_FOOTNOTES, EXTEND_NOTES, EXTEND_NOTES_UNTIL_HEADING,
    FB2_ENDNOTES, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE, FootnoteSize, NOTE_BLOCK_SPACING,
    NOTE_INDENTATION, NOTE_INLINE_SIZING, NOTE_JUSTIFICATION, PRESETS, Snapshot,
};
use primitive_ui::{TextStyle, editor::Multiline};
#[derive(Clone)]
pub(super) enum Command {
    Preset(String),
    Choose(String, Option<bool>),
    Global(String, bool),
    SetFootnoteSize(Option<FootnoteSize>),
    Master(bool),
    Files,
    Import(usize),
    Edit,
    Review,
    Apply,
    ToggleCustom,
    Clear,
    ConfirmClear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteSizeScope {
    Book,
    Default,
}
#[derive(Default)]
pub(super) struct View {
    owner: Option<u64>,
    preset: Option<String>,
    files: Vec<PathBuf>,
    draft: Option<String>,
    error: Option<String>,
    pub(super) editor: Option<Multiline>,
}
const CANCEL: Rect = Rect::new(40, 126, 550, 104);
const REVIEW: Rect = Rect::new(674, 126, 550, 104);
const TEXT: Rect = Rect::new(40, 260, 1184, 480);
const LINE_HEIGHT: i32 = 48;
// Use the same wrap in both orientations so that an open draft keeps its line.
const COLUMNS: usize = 40;

fn note_format(preset: &str) -> Option<DocumentFormat> {
    match preset {
        EPUB_FOOTNOTES | CLASSIC_FOOTNOTES => Some(DocumentFormat::Epub),
        FB2_FOOTNOTES | FB2_ENDNOTES | FB2_REGULAR_NOTE_SIZE => Some(DocumentFormat::Fb2),
        _ => None,
    }
}

fn advanced_note(preset: &str) -> bool {
    matches!(
        preset,
        CLASSIC_FOOTNOTES
            | EXTEND_NOTES
            | EXTEND_NOTES_UNTIL_HEADING
            | NOTE_INDENTATION
            | NOTE_JUSTIFICATION
            | NOTE_BLOCK_SPACING
            | NOTE_INLINE_SIZING
    )
}

fn preset_item(preset: user_styles::Preset, data: &Data, book: &user_styles::BookTweaks) -> Item {
    let source = match book.choices.get(preset.id) {
        Some(true) => "On for this book",
        Some(false) => "Off for this book",
        None if data.global.contains(preset.id) => "Use default: On",
        None => "Use default: Off",
    };
    Item::new(
        preset.title,
        Action::Style(Command::Preset(preset.id.into())),
    )
    .detail(source)
}

struct EditorLayout {
    title: Rect,
    cancel: Rect,
    review: Rect,
    text: Rect,
    hint: Rect,
    keyboard: Layout,
    rows: usize,
}

impl EditorLayout {
    fn new(layout: NavigationLayout) -> Self {
        let keyboard = layout.keyboard();
        let hint_y = 756.min(keyboard.keyboard_area().y - 60);
        let rows = ((hint_y - 16 - TEXT.y) / LINE_HEIGHT).max(1) as usize;
        let extra_half_width = (layout.width - NavigationLayout::PORTRAIT.width) / 2;
        Self {
            title: layout.body(35, 72),
            cancel: Rect::new(CANCEL.x, CANCEL.y, CANCEL.w + extra_half_width, CANCEL.h),
            review: Rect::new(
                REVIEW.x + extra_half_width,
                REVIEW.y,
                REVIEW.w + extra_half_width,
                REVIEW.h,
            ),
            text: layout.body(TEXT.y, rows as i32 * LINE_HEIGHT),
            hint: layout.body(hint_y, 60),
            keyboard,
            rows,
        }
    }
}

impl Reader {
    fn style_book(&self) -> Result<u64> {
        self.active
            .as_ref()
            .map(|b| b.id)
            .ok_or_else(|| "Open a reflowable book first".into())
    }
    fn style_preset_available(&self, name: &str) -> bool {
        let format = note_format(name);
        if format.is_none() && !advanced_note(name) {
            return true;
        }
        self.active.as_ref().is_some_and(|book| {
            format.map_or(
                matches!(
                    book.entry.format,
                    DocumentFormat::Epub | DocumentFormat::Fb2
                ),
                |required| book.entry.format == required,
            ) && self.style_store.data.book(book.id).enabled
        })
    }

    pub(super) fn style_preset_parent(&self) -> Screen {
        if self.style_view.preset.as_deref().is_some_and(advanced_note) {
            Screen::FootnoteTweaks
        } else {
            Screen::StyleTweaks
        }
    }

    fn apply_style_data(&mut self, next: Data) -> Result<()> {
        let id = self.style_book()?;
        next.validate()?;
        let active = self.active.as_ref().unwrap();
        let snapshot = next.snapshot(id)?;
        let candidate = if snapshot == active.reading.config.user_styles {
            None
        } else {
            let mut config = active.reading.config.clone();
            config.user_styles = snapshot;
            config.prepare_page_layout(&active.reading.config);
            let anchor = active.reading.anchor();
            let candidate = Reading::restore_with_scroll_top(
                config,
                active.reading.chapter,
                active.reading.page,
                anchor.as_ref(),
                active.reading.scroll_history_top(),
            )?;
            if candidate.probe.book != active.reading.probe.book
                || anchor.is_some() && candidate.anchor() != anchor
            {
                return Err("The reading location changed; reopen the book before applying".into());
            }
            Some(candidate)
        };
        // A failed write leaves the visible page, stored model and old snapshot intact.
        self.style_store.save(next)?;
        if let Some(candidate) = candidate {
            self.active.as_mut().unwrap().reading = candidate;
        }
        self.record_position();
        self.notice = Some("Style tweaks saved".into());
        Ok(())
    }
    pub(super) fn activate_style(&mut self, command: Command) -> Result<()> {
        let id = self.style_book()?;
        if self.style_view.owner.is_some_and(|owner| owner != id)
            && !matches!(
                &command,
                Command::Preset(_) | Command::Files | Command::Edit
            )
        {
            return Err("The open book changed; reopen style tweaks before applying".into());
        }
        if matches!(
            &command,
            Command::Apply | Command::Review | Command::Import(_) | Command::ConfirmClear
        ) && self.style_view.owner != Some(id)
        {
            return Err("Review this book's styles again before applying".into());
        }
        if let Command::Preset(name) | Command::Choose(name, _) | Command::Global(name, _) =
            &command
            && !self.style_preset_available(name)
        {
            return Err("Enable style tweaks in a supported reflowable book first".into());
        }
        match command {
            Command::Preset(name) => {
                self.style_view.owner = Some(id);
                user_styles::preset(&name).ok_or("This style tweak is unavailable")?;
                self.style_view.preset = Some(name);
                self.show(Screen::StylePreset);
            }
            Command::Choose(name, value) => {
                let mut next = self.style_store.data.clone();
                next.choose(id, &name, value)?;
                self.apply_style_data(next)?;
            }
            Command::Global(name, value) => {
                let mut next = self.style_store.data.clone();
                next.make_global(id, &name, value)?;
                self.apply_style_data(next)?;
            }
            Command::SetFootnoteSize(value) => {
                if !self.style_preset_available(EPUB_FOOTNOTES) {
                    return Err("Enable style tweaks in a reflowable EPUB first".into());
                }
                let mut next = self.style_store.data.clone();
                match self.screen {
                    Screen::InPageFootnoteSize(NoteSizeScope::Book) => {
                        next.books.entry(id).or_default().footnote_size = value;
                    }
                    Screen::InPageFootnoteSize(NoteSizeScope::Default) => {
                        next.footnote_size = value.ok_or("Choose a default size first")?;
                    }
                    _ => return Err("Open the in-page footnote size settings first".into()),
                }
                self.apply_style_data(next)?;
            }
            Command::Master(enabled) => {
                let mut next = self.style_store.data.clone();
                next.books.entry(id).or_default().enabled = enabled;
                self.apply_style_data(next)?;
            }
            Command::Files => {
                self.style_view.owner = Some(id);
                let directory = self.style_store.path.parent().unwrap().join("style-tweaks");
                self.style_view.files = with_storage(|| {
                    fs::create_dir_all(&directory)?;
                    if !fs::symlink_metadata(&directory)?.file_type().is_dir() {
                        return Err(io::Error::other(
                            "Style import folder must be a regular directory",
                        ));
                    }
                    let mut files = Vec::new();
                    for entry in fs::read_dir(&directory)? {
                        let entry = entry?;
                        if entry.file_type()?.is_file()
                            && entry
                                .path()
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("css"))
                        {
                            files.push(entry.path());
                            if files.len() > 64 {
                                return Err(io::Error::other(
                                    "The style import folder exceeds 64 CSS files",
                                ));
                            }
                        }
                    }
                    files.sort();
                    Ok(files)
                })?;
                self.style_view.editor = None;
                self.show(Screen::StyleFiles);
            }
            Command::Import(index) => {
                let path = self
                    .style_view
                    .files
                    .get(index)
                    .ok_or("Choose a CSS file again")?;
                let bytes = read_import_file(path, user_styles::MAX_BOOK_CSS as u64)?;
                let css = String::from_utf8(bytes).map_err(|_| "CSS must be valid UTF-8")?;
                self.review_styles(css);
            }
            Command::Edit => {
                self.style_view.owner = Some(id);
                let text = self.style_store.data.book(id).custom.unwrap_or_default();
                let mut editor = Multiline::new(&text, user_styles::MAX_BOOK_CSS);
                editor.editor.caret = editor.editor.text.len();
                self.style_view.editor = Some(editor);
                self.show(Screen::StyleEditor);
            }
            Command::Review => {
                let text = self
                    .style_view
                    .editor
                    .as_ref()
                    .ok_or("Open the CSS editor again")?
                    .editor
                    .string();
                self.review_styles(text);
            }
            Command::Apply => {
                let text = self
                    .style_view
                    .draft
                    .clone()
                    .ok_or("Review the CSS again")?;
                if text.len() > user_styles::MAX_BOOK_CSS {
                    return Err("Book CSS exceeds 8 KiB; the saved CSS is unchanged".into());
                }
                Snapshot::new(text.clone())?;
                let mut next = self.style_store.data.clone();
                let b = next.books.entry(id).or_default();
                let new = b.custom.is_none();
                b.custom = (!text.trim().is_empty()).then_some(text);
                if b.custom.is_none() {
                    b.custom_enabled = false;
                } else if new {
                    b.custom_enabled = true;
                }
                self.apply_style_data(next)?;
                self.style_view.draft = None;
                self.style_view.editor = None;
                self.show(Screen::Book);
            }
            Command::ToggleCustom => {
                let mut next = self.style_store.data.clone();
                let b = next.books.entry(id).or_default();
                if b.custom.is_none() {
                    return Err("Import or edit book CSS first".into());
                }
                b.custom_enabled = !b.custom_enabled;
                self.apply_style_data(next)?;
            }
            Command::Clear => {
                self.style_view.owner = Some(id);
                self.show(Screen::StyleClear);
            }
            Command::ConfirmClear => {
                let mut next = self.style_store.data.clone();
                let b = next.books.entry(id).or_default();
                b.custom = None;
                b.custom_enabled = false;
                self.apply_style_data(next)?;
                self.show(Screen::StyleTweaks);
            }
        }
        Ok(())
    }
    fn review_styles(&mut self, css: String) {
        self.style_view.error = if css.len() > user_styles::MAX_BOOK_CSS {
            Some("Book CSS exceeds 8 KiB; shorten it before applying.".into())
        } else {
            Snapshot::new(css.clone())
                .err()
                .map(|error| error.to_string())
        };
        self.style_view.draft = Some(css);
        self.show(Screen::StyleReview);
    }
    pub(super) fn style_items(&self, screen: Screen) -> Vec<Item> {
        let Ok(id) = self.style_book() else {
            return vec![Item::new("Open a reflowable book first", Action::Heading)];
        };
        let data = &self.style_store.data;
        let b = data.book(id);
        let action = |title: &str, command: Command| Item::new(title, Action::Style(command));
        match screen {
            Screen::StyleTweaks => {
                let mut items = vec![
                    action("Enable style tweaks", Command::Master(!b.enabled)).detail(
                        if b.enabled {
                            "On for this book"
                        } else {
                            "Off; choices and CSS retained"
                        },
                    ),
                    action("Edit book CSS", Command::Edit)
                        .detail("Supported CSS only; review before applying"),
                    action("Import book CSS", Command::Files)
                        .detail("Copy a .css file to reader/data/style-tweaks"),
                ];
                if b.custom.is_some() {
                    items.push(action("Book CSS", Command::ToggleCustom).detail(
                        if b.custom_enabled {
                            "On"
                        } else {
                            "Off; text retained"
                        },
                    ));
                    items.push(action("Clear book CSS", Command::Clear));
                }
                if let Some(problem) = &self.style_store.problem {
                    items.push(
                        Item::new("Saved settings preserved", Action::Heading).detail(problem),
                    );
                }
                let unavailable: std::collections::BTreeSet<_> = data
                    .global
                    .iter()
                    .chain(b.choices.keys())
                    .filter(|id| user_styles::preset(id).is_none())
                    .collect();
                if !unavailable.is_empty() {
                    items.push(
                        Item::new("Unavailable tweaks retained", Action::Heading).detail(format!(
                            "{} saved IDs are not supported by this build",
                            unavailable.len()
                        )),
                    );
                }
                let mut note_row = 1;
                for p in PRESETS {
                    if advanced_note(p.id) {
                        continue;
                    }
                    let format = note_format(p.id);
                    if let Some(format) = format
                        && self
                            .active
                            .as_ref()
                            .is_none_or(|book| book.entry.format != format)
                    {
                        continue;
                    }
                    if !self.style_preset_available(p.id) {
                        items.insert(
                            note_row,
                            Item::new(p.title, Action::Heading)
                                .detail("Enable style tweaks to change this setting"),
                        );
                        note_row += 1;
                        continue;
                    }
                    let row = preset_item(*p, data, &b);
                    if format.is_some() {
                        items.insert(note_row, row);
                        note_row += 1;
                    } else {
                        items.push(row);
                    }
                    if p.id == EPUB_FOOTNOTES {
                        items.insert(
                            note_row,
                            Item::new(
                                "In-page footnote size",
                                Action::Show(Screen::InPageFootnoteSize(NoteSizeScope::Book)),
                            )
                            .detail(b.footnote_size.unwrap_or(data.footnote_size).label()),
                        );
                        note_row += 1;
                    }
                }
                if b.enabled
                    && self.active.as_ref().is_some_and(|book| {
                        matches!(
                            book.entry.format,
                            DocumentFormat::Epub | DocumentFormat::Fb2
                        )
                    })
                {
                    items.insert(
                        note_row,
                        Item::new(
                            "More footnote options",
                            Action::Show(Screen::FootnoteTweaks),
                        ),
                    );
                }
                items.push(Item::new("Supported reflow styles",Action::Heading).detail("Margins, hiding, floats, arbitrary selectors and vendor hints are not supported."));
                items
            }
            Screen::FootnoteTweaks => PRESETS
                .iter()
                .copied()
                .filter(|preset| advanced_note(preset.id) && self.style_preset_available(preset.id))
                .map(|preset| preset_item(preset, data, &b))
                .collect(),
            Screen::InPageFootnoteSize(scope) => {
                let mut items = Vec::new();
                let selected = match scope {
                    NoteSizeScope::Book => {
                        items.push(
                            action("Use default", Command::SetFootnoteSize(None)).detail(
                                if b.footnote_size.is_none() {
                                    format!("Selected · {}", data.footnote_size.label())
                                } else {
                                    data.footnote_size.label().into()
                                },
                            ),
                        );
                        items.push(Item::new(
                            "Default size for all books",
                            Action::Show(Screen::InPageFootnoteSize(NoteSizeScope::Default)),
                        ));
                        b.footnote_size
                    }
                    NoteSizeScope::Default => Some(data.footnote_size),
                };
                items.extend(FootnoteSize::CHOICES.into_iter().map(|size| {
                    let item = action(size.label(), Command::SetFootnoteSize(Some(size)));
                    if selected == Some(size) {
                        item.detail("Selected")
                    } else {
                        item
                    }
                }));
                items
            }
            Screen::StylePreset => {
                let Some(p) = self
                    .style_view
                    .preset
                    .as_deref()
                    .and_then(user_styles::preset)
                else {
                    return Vec::new();
                };
                let current = b.choices.get(p.id).copied();
                let global = data.global.contains(p.id);
                let description = match p.id {
                    EPUB_FOOTNOTES => "Show marked notes in supported single-page EPUB layouts",
                    CLASSIC_FOOTNOTES => "Use common publisher footnote classes in this EPUB",
                    FB2_FOOTNOTES => "Show linked sections from the FictionBook notes body",
                    FB2_ENDNOTES => "Show linked sections from the FictionBook comments body",
                    FB2_REGULAR_NOTE_SIZE => "Use the reading size for FictionBook note bands",
                    EXTEND_NOTES => {
                        "Include following blocks up to the next note or end of the group"
                    }
                    EXTEND_NOTES_UNTIL_HEADING => "Also stop at a following heading",
                    NOTE_INDENTATION => "Remove the first-line indent from notes",
                    NOTE_JUSTIFICATION => "Justify the lines of the note text",
                    NOTE_BLOCK_SPACING => "Remove margins and padding around note blocks",
                    NOTE_INLINE_SIZING => "Use inherited note text size, line height and baseline",
                    _ => "Book choice overrides the live all-books default",
                };
                let detail = if note_format(p.id).is_some() {
                    "Apply to eligible books that use the default"
                } else {
                    "Existing books inherit unless they override it"
                };
                let status = if note_format(p.id).is_some() || advanced_note(p.id) {
                    Item::new("When notes appear in the page", Action::Heading)
                        .detail("Horizontal reading with marked notes in the same chapter")
                } else {
                    Item::new("Reader settings still apply", Action::Heading)
                        .detail("Explicit alignment, font override and line spacing remain independent controls.")
                };
                vec![
                    Item::new(p.title, Action::Heading).detail(description),
                    action("Use default", Command::Choose(p.id.into(), None)).detail(
                        if current.is_none() {
                            "Selected"
                        } else {
                            "Inherit future all-books changes"
                        },
                    ),
                    action("On for this book", Command::Choose(p.id.into(), Some(true))).detail(
                        if current == Some(true) {
                            "Selected"
                        } else {
                            "Override the default"
                        },
                    ),
                    action(
                        "Off for this book",
                        Command::Choose(p.id.into(), Some(false)),
                    )
                    .detail(if current == Some(false) {
                        "Selected"
                    } else {
                        "Override the default"
                    }),
                    action(
                        if global {
                            "Stop using on all books"
                        } else {
                            "Use on all books"
                        },
                        Command::Global(p.id.into(), !global),
                    )
                    .detail(detail),
                    status,
                ]
            }
            Screen::StyleFiles => {
                let mut items: Vec<_> = self
                    .style_view
                    .files
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        action(
                            p.file_name().unwrap().to_string_lossy().as_ref(),
                            Command::Import(i),
                        )
                        .detail("Review a book-specific copy; original file stays unchanged")
                    })
                    .collect();
                if items.is_empty() {
                    items.push(
                        Item::new("No CSS files found", Action::Heading)
                            .detail("Copy UTF-8 CSS files to reader/data/style-tweaks"),
                    );
                }
                items
            }
            Screen::StyleReview => {
                let css = self.style_view.draft.as_deref().unwrap_or("");
                let mut items = if let Some(error) = &self.style_view.error {
                    vec![Item::new("CSS cannot be applied", Action::Heading).detail(error)]
                } else {
                    let summary = self
                        .active
                        .as_ref()
                        .and_then(|a| {
                            crate::book_style::user_css_matches(
                                css,
                                &a.reading.probe.source.elements,
                            )
                            .ok()
                        })
                        .map_or_else(
                            || "Supported CSS".into(),
                            |(rules, elements)| {
                                format!("{rules} rules match {elements} elements in this chapter")
                            },
                        );
                    vec![action("Apply to this book", Command::Apply).detail(summary)]
                };
                if self.style_view.editor.is_some() {
                    items.push(Item::new(
                        "Return to editor",
                        Action::Show(Screen::StyleEditor),
                    ));
                }
                for (number, line) in css.lines().enumerate() {
                    let chars: Vec<_> = line.chars().collect();
                    if chars.is_empty() {
                        items.push(Item::new(format!("{}: ", number + 1), Action::Heading));
                    }
                    for (part, chunk) in chars.chunks(28).enumerate() {
                        items.push(Item::new(
                            format!(
                                "{}{} {}",
                                number + 1,
                                if part == 0 { ":" } else { "+" },
                                chunk.iter().collect::<String>()
                            ),
                            Action::Heading,
                        ));
                    }
                }
                items
            }
            Screen::StyleClear => vec![
                action("Clear this book's CSS", Command::ConfirmClear)
                    .detail("Built-in choices and all-books defaults stay unchanged"),
                Item::new("Cancel", Action::Show(Screen::StyleTweaks)),
            ],
            _ => Vec::new(),
        }
    }
    pub(super) fn draw_style_editor(&self, canvas: &mut Canvas) -> bool {
        if self.screen != Screen::StyleEditor {
            return false;
        }
        let layout = EditorLayout::new(self.navigation_layout());
        canvas.text(layout.title, "Book CSS", TextStyle::new(48).bold());
        canvas.button(layout.cancel, "Cancel", false, false);
        canvas.button(layout.review, "Review", true, false);
        if let Some(editor) = &self.style_view.editor {
            editor.draw(canvas, layout.text, COLUMNS, LINE_HEIGHT, 28);
            canvas.keyboard(editor.editor.layer, "New line", None);
        }
        canvas.text(
            layout.hint,
            "8 KiB limit. Unsupported CSS is rejected before saving.",
            CAPTION,
        );
        true
    }
    pub(super) fn style_editor_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::StyleEditor || matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let layout = EditorLayout::new(self.navigation_layout());
        let result = (|| -> Result<Change> {
            if let Input::Tap(x, y) = input {
                if layout.cancel.contains(x, y) {
                    self.style_view.editor = None;
                    self.show(Screen::StyleTweaks);
                    return Ok(Change::Partial);
                }
                if layout.review.contains(x, y) {
                    self.activate_style(Command::Review)?;
                    return Ok(Change::Partial);
                }
                let editor = self
                    .style_view
                    .editor
                    .as_mut()
                    .ok_or("Open the editor again")?;
                if layout.text.contains(x, y) {
                    editor.place(
                        ((y - layout.text.y as f32) / LINE_HEIGHT as f32) as usize,
                        ((x - layout.text.x as f32) / 18.) as usize,
                        COLUMNS,
                    );
                } else if let Some(key) =
                    keyboard_key(&layout.keyboard, &editor.editor, "New line", x, y)
                {
                    editor.press(key);
                    editor.show_caret(COLUMNS, layout.rows);
                } else {
                    return Ok(Change::None);
                }
            } else {
                let delta = match input {
                    Input::PageForward => 1,
                    Input::PageBack => -1,
                    Input::Swipe { dy, .. } if dy.abs() > 60. => {
                        if dy < 0. {
                            1
                        } else {
                            -1
                        }
                    }
                    _ => return Ok(Change::None),
                };
                if let Some(editor) = &mut self.style_view.editor {
                    editor.page(delta, COLUMNS, layout.rows);
                }
            }
            Ok(Change::Partial)
        })();
        Some(match result {
            Ok(change) => change,
            Err(error) => self.report(error),
        })
    }
}

#[cfg(test)]
mod tests;
