//! Supported style controls, book-specific CSS review and multiline editing.
use super::navigation_layout::NavigationLayout;
use super::*;
use crate::user_styles::{
    self, CLASSIC_FOOTNOTES, COMBINE_NONLINEAR_NOTES, Data, EPUB_FOOTNOTES, EXTEND_NOTES,
    EXTEND_NOTES_UNTIL_HEADING, FB2_ENDNOTES, FB2_FOOTNOTES, FB2_REGULAR_NOTE_SIZE, FootnoteSize,
    INLINE_ALL_FOOTNOTE_CONTENT, NOTE_BLOCK_SPACING, NOTE_COSMETIC_SPACING, NOTE_INDENTATION,
    NOTE_INLINE_SIZING, NOTE_JUSTIFICATION, PRESETS, Snapshot, WIKIPEDIA_FOOTNOTES,
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
const PARAGRAPH_SPACING: &str = "paragraph-spacing";
const PSEUDO_ELEMENTS: &str = "pseudo-elements";
const LIST_MARKER: &str = "list-marker-";
const TABLE_STYLES: [&str; 8] = [
    "table_no_presentational_hints",
    "table_full_width",
    "table_td_width_auto",
    "table_margin_left_right_auto",
    "td_vertical_align_none",
    "table_row_odd_even",
    "table_force_border",
    "table-visible-borders",
];

fn note_format(preset: &str) -> Option<DocumentFormat> {
    match preset {
        EPUB_FOOTNOTES | CLASSIC_FOOTNOTES | WIKIPEDIA_FOOTNOTES | COMBINE_NONLINEAR_NOTES => {
            Some(DocumentFormat::Epub)
        }
        FB2_FOOTNOTES | FB2_ENDNOTES | FB2_REGULAR_NOTE_SIZE => Some(DocumentFormat::Fb2),
        _ => None,
    }
}

fn supports_note_format(preset: &str, format: DocumentFormat) -> bool {
    note_format(preset).map_or(
        matches!(
            format,
            DocumentFormat::Epub | DocumentFormat::Fb2 | DocumentFormat::Fb3
        ),
        |required| {
            format == required
                || (format == DocumentFormat::Fb3
                    && matches!(preset, FB2_FOOTNOTES | FB2_REGULAR_NOTE_SIZE))
        },
    )
}

fn book_preset(mut preset: user_styles::Preset, format: DocumentFormat) -> user_styles::Preset {
    if format == DocumentFormat::Fb3 {
        preset.title = match preset.id {
            FB2_FOOTNOTES => "In-page FB3 footnotes",
            FB2_REGULAR_NOTE_SIZE => "Keep regular FB3 note size",
            _ => preset.title,
        };
    }
    preset
}

fn advanced_note(preset: &str) -> bool {
    matches!(
        preset,
        CLASSIC_FOOTNOTES
            | WIKIPEDIA_FOOTNOTES
            | EXTEND_NOTES
            | EXTEND_NOTES_UNTIL_HEADING
            | NOTE_INDENTATION
            | NOTE_JUSTIFICATION
            | NOTE_BLOCK_SPACING
            | NOTE_COSMETIC_SPACING
            | INLINE_ALL_FOOTNOTE_CONTENT
            | NOTE_INLINE_SIZING
            | COMBINE_NONLINEAR_NOTES
    )
}

fn image_preset(preset: &user_styles::Preset) -> bool {
    matches!(preset.group, "images" | "image-alignment")
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
            supports_note_format(name, book.reading_format())
                && self.style_store.data.book(book.id).enabled
        })
    }

    pub(super) fn style_preset_parent(&self) -> Screen {
        match self
            .style_view
            .preset
            .as_deref()
            .and_then(user_styles::preset)
        {
            Some(preset) if advanced_note(preset.id) => Screen::FootnoteTweaks,
            Some(preset) if preset.group == PARAGRAPH_SPACING => Screen::ParagraphSpacing,
            Some(preset) if preset.group.starts_with("link-") => Screen::LinkStyles,
            Some(preset) if image_preset(&preset) => Screen::ImageStyles,
            Some(preset) if TABLE_STYLES.contains(&preset.id) => Screen::TableStyles,
            Some(preset) if preset.group.starts_with(PSEUDO_ELEMENTS) => {
                Screen::PseudoElementStyles
            }
            Some(preset) if preset.group.starts_with(LIST_MARKER) => Screen::ListItemStyles,
            Some(preset) if preset.group == "normal-line-height" => Screen::NormalLineHeightStyles,
            _ => Screen::StyleTweaks,
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
        let language = self.controls.interface_language;
        let Some(book) = &self.active else {
            return vec![Item::new(
                language.bottom_typography_text("Open a reflowable book first"),
                Action::Heading,
            )];
        };
        let data = &self.style_store.data;
        let b = data.book(book.id);
        let format = book.reading_format();
        let action = |title: &str, command: Command| Item::new(title, Action::Style(command));
        match screen {
            Screen::StyleTweaks => {
                let mut items = vec![
                    action(
                        language.bottom_typography_text("Enable style tweaks"),
                        Command::Master(!b.enabled),
                    )
                    .detail(if b.enabled {
                        language.bottom_typography_text("On for this book")
                    } else {
                        language.bottom_typography_text("Off; choices and CSS retained")
                    }),
                    action(
                        language.bottom_typography_text("Edit book CSS"),
                        Command::Edit,
                    )
                    .detail(
                        language
                            .bottom_typography_text("Supported CSS only; review before applying"),
                    ),
                    action(
                        language.bottom_typography_text("Import book CSS"),
                        Command::Files,
                    )
                    .detail(
                        language
                            .bottom_typography_text("Copy a .css file to reader/data/style-tweaks"),
                    ),
                ];
                if b.custom.is_some() {
                    items.push(
                        action(
                            language.bottom_typography_text("Book CSS"),
                            Command::ToggleCustom,
                        )
                        .detail(if b.custom_enabled {
                            language.bottom_typography_text("On")
                        } else {
                            language.bottom_typography_text("Off; text retained")
                        }),
                    );
                    items.push(action(
                        language.bottom_typography_text("Clear book CSS"),
                        Command::Clear,
                    ));
                }
                if let Some(problem) = &self.style_store.problem {
                    items.push(
                        Item::new(
                            language.bottom_typography_text("Saved settings preserved"),
                            Action::Heading,
                        )
                        .detail(problem),
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
                        Item::new(
                            language.bottom_typography_text("Unavailable tweaks retained"),
                            Action::Heading,
                        )
                        .detail(if language.is_english() {
                            format!(
                                "{} saved IDs are not supported by this build",
                                unavailable.len()
                            )
                        } else {
                            format!(
                                "{} ID đã lưu không được bản dựng này hỗ trợ",
                                unavailable.len()
                            )
                        }),
                    );
                }
                let mut note_row = 1;
                for p in PRESETS {
                    if p.group.starts_with("link-") {
                        if p.id == "link-black" {
                            items.push(Item::new(
                                language.bottom_typography_text("Links"),
                                Action::Show(Screen::LinkStyles),
                            ));
                        }
                        continue;
                    }
                    if p.group == PARAGRAPH_SPACING {
                        if p.id == "paragraph_whitespace" {
                            let state = PRESETS
                                .iter()
                                .find(|p| {
                                    p.group == PARAGRAPH_SPACING && data.enabled(book.id, p.id)
                                })
                                .map_or("Use book spacing", |p| match p.id {
                                    "paragraph_whitespace" => "Full",
                                    "paragraph_whitespace_half" => "Half",
                                    _ => "None",
                                });
                            items.push(
                                Item::new(
                                    language.bottom_typography_text("Spacing between paragraphs"),
                                    Action::Show(Screen::ParagraphSpacing),
                                )
                                .detail(language.bottom_typography_text(state)),
                            );
                        }
                        continue;
                    }
                    if image_preset(p) {
                        if p.id == "image_full_width" {
                            items.push(Item::new(
                                language.bottom_typography_text("Images"),
                                Action::Show(Screen::ImageStyles),
                            ));
                        }
                        continue;
                    }
                    if TABLE_STYLES.contains(&p.id) {
                        if p.id == TABLE_STYLES[0] {
                            items.push(Item::new(
                                language.bottom_typography_text("Tables"),
                                Action::Show(Screen::TableStyles),
                            ));
                        }
                        continue;
                    }
                    if p.group.starts_with(PSEUDO_ELEMENTS) {
                        if p.id == "no_pseudo_element_before_after" {
                            items.push(Item::new(
                                language.bottom_typography_text("Pseudo-elements"),
                                Action::Show(Screen::PseudoElementStyles),
                            ));
                        }
                        continue;
                    }
                    if p.group.starts_with(LIST_MARKER) {
                        if p.id == "ul_li_type_disc" {
                            items.push(Item::new(
                                language.bottom_typography_text("List items"),
                                Action::Show(Screen::ListItemStyles),
                            ));
                        }
                        continue;
                    }
                    if p.group == "normal-line-height" {
                        if p.id == "normal_line-height_90" {
                            let selected = PRESETS
                                .iter()
                                .find(|preset| {
                                    preset.group == p.group && data.enabled(book.id, preset.id)
                                })
                                .map_or("Use font metrics", |preset| preset.title);
                            items.push(
                                Item::new(
                                    language.bottom_typography_text("Normal line height"),
                                    Action::Show(Screen::NormalLineHeightStyles),
                                )
                                .detail(selected),
                            );
                        }
                        continue;
                    }
                    if advanced_note(p.id) {
                        continue;
                    }
                    let is_note = note_format(p.id).is_some();
                    if is_note && !supports_note_format(p.id, format) {
                        continue;
                    }
                    let p = book_preset(*p, format);
                    if !self.style_preset_available(p.id) {
                        items.insert(
                            note_row,
                            Item::new(p.title, Action::Heading)
                                .detail("Enable style tweaks to change this setting"),
                        );
                        note_row += 1;
                        continue;
                    }
                    let row = preset_item(p, data, &b);
                    if is_note {
                        items.insert(note_row, row);
                        note_row += 1;
                    } else {
                        items.push(row);
                    }
                    if p.id == EPUB_FOOTNOTES {
                        items.insert(
                            note_row,
                            Item::new(
                                language.bottom_typography_text("In-page footnote size"),
                                Action::Show(Screen::InPageFootnoteSize(NoteSizeScope::Book)),
                            )
                            .detail(b.footnote_size.unwrap_or(data.footnote_size).label()),
                        );
                        note_row += 1;
                    }
                }
                if b.enabled && supports_note_format(INLINE_ALL_FOOTNOTE_CONTENT, format) {
                    items.insert(
                        note_row,
                        Item::new(
                            language.bottom_typography_text("More footnote options"),
                            Action::Show(Screen::FootnoteTweaks),
                        ),
                    );
                }
                items.push(Item::new(language.bottom_typography_text("Supported reflow styles"),Action::Heading).detail(language.bottom_typography_text("Margins, hiding, floats, arbitrary selectors and vendor hints are not supported.")));
                items
            }
            Screen::ParagraphSpacing => PRESETS
                .iter()
                .filter(|preset| preset.group == PARAGRAPH_SPACING)
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
            Screen::LinkStyles => PRESETS
                .iter()
                .filter(|preset| preset.group.starts_with("link-"))
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
            Screen::ImageStyles => PRESETS
                .iter()
                .filter(|preset| image_preset(preset))
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
            Screen::TableStyles => TABLE_STYLES
                .iter()
                .filter_map(|id| user_styles::preset(id))
                .map(|preset| preset_item(preset, data, &b))
                .collect(),
            Screen::PseudoElementStyles => PRESETS
                .iter()
                .filter(|preset| preset.group.starts_with(PSEUDO_ELEMENTS))
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
            Screen::ListItemStyles => PRESETS
                .iter()
                .filter(|preset| preset.group.starts_with(LIST_MARKER))
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
            Screen::NormalLineHeightStyles => PRESETS
                .iter()
                .filter(|preset| preset.group == "normal-line-height")
                .map(|preset| preset_item(*preset, data, &b))
                .collect(),
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
                    .map(|preset| book_preset(preset, format))
                else {
                    return Vec::new();
                };
                let current = b.choices.get(p.id).copied();
                let global = data.global.contains(p.id);
                let description = match p.id {
                    "link-black" => "Use black ink for links and anchors",
                    "link-blue" => "Use the Kindle grayscale shade for blue links",
                    "table_no_presentational_hints" => {
                        "Use stylesheet rules without layout hints from HTML attributes"
                    }
                    "td_vertical_align_none" => "Align body cells to the row’s first text baseline",
                    "no_pseudo_element_before_after" => {
                        "Hide generated text before or after the original content"
                    }
                    "no_pseudo_element_first_letter" => {
                        "Suppress the publisher’s first-letter styling"
                    }
                    "no_pseudo_element_first_line" => "Suppress the publisher’s first-line styling",
                    "paragraph_web_browser_style" => {
                        "Use block paragraphs with no default first-line indent"
                    }
                    "paragraph_whitespace" => "Add one em between consecutive paragraphs",
                    "paragraph_whitespace_half" => "Add half an em between consecutive paragraphs",
                    "paragraph_no_whitespace" => "Remove top and bottom margins from paragraphs",
                    EPUB_FOOTNOTES => "Show marked notes in supported single-page EPUB layouts",
                    CLASSIC_FOOTNOTES => "Use common publisher footnote classes in this EPUB",
                    WIKIPEDIA_FOOTNOTES => "Use reference lists in a saved Wikipedia EPUB",
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
                    NOTE_COSMETIC_SPACING => "Remove margins and padding on inline note elements",
                    INLINE_ALL_FOOTNOTE_CONTENT => {
                        "Combine simple note paragraphs with their number"
                    }
                    NOTE_INLINE_SIZING => "Use inherited note text size, line height and baseline",
                    COMBINE_NONLINEAR_NOTES => "Group original notes for Hide non-linear fragments",
                    "image_full_width" => {
                        "Place images on separate lines. Authored height can affect the aspect ratio."
                    }
                    "image_valign_middle" => "Align inline images to the middle of adjoining text",
                    _ => "Book choice overrides the live all-books default",
                };
                let detail = if note_format(p.id).is_some() {
                    language.bottom_typography_text("Apply to eligible books that use the default")
                } else {
                    language
                        .bottom_typography_text("Existing books inherit unless they override it")
                };
                let status = if note_format(p.id).is_some() || advanced_note(p.id) {
                    Item::new(
                        language.bottom_typography_text("When notes appear in the page"),
                        Action::Heading,
                    )
                    .detail(language.bottom_typography_text(
                        "Horizontal reading with marked notes in the same chapter",
                    ))
                } else {
                    Item::new(language.bottom_typography_text("Reader settings still apply"), Action::Heading)
                        .detail(language.bottom_typography_text("Explicit alignment, font override and line spacing remain independent controls."))
                };
                vec![
                    Item::new(p.title, Action::Heading).detail(description),
                    action(
                        language.bottom_typography_text("Use default"),
                        Command::Choose(p.id.into(), None),
                    )
                    .detail(if current.is_none() {
                        language.bottom_typography_text("Selected")
                    } else {
                        language.bottom_typography_text("Inherit future all-books changes")
                    }),
                    action(
                        language.bottom_typography_text("On for this book"),
                        Command::Choose(p.id.into(), Some(true)),
                    )
                    .detail(if current == Some(true) {
                        language.bottom_typography_text("Selected")
                    } else {
                        language.bottom_typography_text("Override the default")
                    }),
                    action(
                        language.bottom_typography_text("Off for this book"),
                        Command::Choose(p.id.into(), Some(false)),
                    )
                    .detail(if current == Some(false) {
                        language.bottom_typography_text("Selected")
                    } else {
                        language.bottom_typography_text("Override the default")
                    }),
                    action(
                        if global {
                            language.bottom_typography_text("Stop using on all books")
                        } else {
                            language.bottom_typography_text("Use on all books")
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
                        .detail(language.bottom_typography_text(
                            "Review a book-specific copy; original file stays unchanged",
                        ))
                    })
                    .collect();
                if items.is_empty() {
                    items.push(
                        Item::new(
                            language.bottom_typography_text("No CSS files found"),
                            Action::Heading,
                        )
                        .detail(language.bottom_typography_text(
                            "Copy UTF-8 CSS files to reader/data/style-tweaks",
                        )),
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
