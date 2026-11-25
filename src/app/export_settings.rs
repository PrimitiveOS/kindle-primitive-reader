//! Local destination and filename preferences shared by annotation file exporters.
use super::*;
mod filename;
use crate::library::{BrowseKind, display_filename};
pub(super) use filename::{Context as FilenameContext, Scope, Subject, Values as FilenameValues};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::MetadataExt;

const SETTINGS_BYTES: u64 = 16 * 1024;
const MAX_NAME_BYTES: usize = 200;
const MAX_COPIES: usize = 999;
const CLEAR_DRAFT: Rect = Rect::new(LIST_X, LIST_Y + 240, 320, 96);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    Book,
    Library,
    Exchange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Settings,
    Folders,
    Template(Scope),
    Tokens,
}

#[derive(Clone)]
pub(super) enum Command {
    Open,
    Browse(PathBuf),
    UseFolder,
    UseDefault,
    ToggleBookFolder,
    ToggleBookName,
    EditTemplate(Scope),
    Tokens,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    folder: Option<PathBuf>,
    pub(super) book_folder: bool,
    pub(super) book_title: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    single_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    multiple_template: Option<String>,
}

impl Preferences {
    pub(super) fn template(&self, scope: Scope) -> Option<&str> {
        match scope {
            Scope::Single => self.single_template.as_deref(),
            Scope::Multiple => self.multiple_template.as_deref(),
        }
    }

    fn store_template(&mut self, scope: Scope, value: String) {
        let field = match scope {
            Scope::Single => &mut self.single_template,
            Scope::Multiple => &mut self.multiple_template,
        };
        *field = (!value.trim().is_empty() && filename::valid_template(&value)).then_some(value);
    }
}

pub(super) struct Settings {
    path: PathBuf,
    pub(super) preferences: Preferences,
    picker: Option<FolderPicker>,
}

struct FolderPicker {
    directory: PathBuf,
    children: Vec<PathBuf>,
    truncated: bool,
}

impl Settings {
    pub(super) fn load(path: PathBuf) -> Self {
        let mut preferences: Preferences = read_import_file(&path, SETTINGS_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for scope in [Scope::Single, Scope::Multiple] {
            if let Some(template) = preferences.template(scope)
                && (template.trim().is_empty() || !filename::valid_template(template))
            {
                preferences.store_template(scope, String::new());
            }
        }
        Self {
            path,
            preferences,
            picker: None,
        }
    }

    pub(super) fn close_picker(&mut self) {
        self.picker = None;
    }

    fn save(&mut self, preferences: Preferences) -> Result<()> {
        let bytes = serde_json::to_vec(&preferences)?;
        if bytes.len() as u64 > SETTINGS_BYTES {
            return Err("Choose a folder with a shorter path".into());
        }
        primitive_app::save(&self.path, &bytes)?;
        self.preferences = preferences;
        Ok(())
    }
}

pub(super) fn folder_label(folder: &Path) -> String {
    let mut label = "Documents".to_owned();
    for part in folder {
        label.push_str(" / ");
        label.push_str(&display_filename(&part.to_string_lossy()));
    }
    label
}

impl Reader {
    pub(super) fn export_settings_items(&self, origin: Origin, page: Page) -> Vec<Item> {
        let language = self.controls.interface_language;
        let row = |title: &str, command| {
            Item::new(
                language.export_settings_text(title),
                Action::ExportSettings(origin, command),
            )
        };
        let settings = &self.export_settings;
        let preferences = &settings.preferences;
        if page == Page::Tokens {
            return filename::GUIDE
                .iter()
                .map(|(tokens, meaning)| Item::new(*tokens, Action::Heading).detail(*meaning))
                .collect();
        }
        if page == Page::Settings {
            let mut rows = vec![
                row(
                    "Choose export folder",
                    Command::Browse(preferences.folder.clone().unwrap_or_default()),
                )
                .detail(preferences.folder.as_deref().map_or_else(
                    || {
                        language
                            .export_settings_text("Reader data / exports")
                            .into()
                    },
                    folder_label,
                )),
                row("Use Reader exports", Command::UseDefault).detail(
                    if preferences.folder.is_none() {
                        language.export_settings_text("Selected")
                    } else {
                        language.export_settings_text("Default folder")
                    },
                ),
                row("Export beside the book", Command::ToggleBookFolder).detail(
                    language.export_settings_text("When one book with a source file is exported"),
                ),
                row("Use book titles for filenames", Command::ToggleBookName).detail(
                    if preferences.book_title {
                        language.export_settings_text("Book title · Field Notes.txt")
                    } else {
                        language.export_settings_text("Book ID · 0123456789abcdef.txt")
                    },
                ),
            ];
            rows.extend([Scope::Single, Scope::Multiple].map(|scope| {
                row(scope.title(), Command::EditTemplate(scope)).detail(
                    preferences
                        .template(scope)
                        .unwrap_or_else(|| language.export_settings_text("Default name")),
                )
            }));
            rows.push(row("Filename tokens", Command::Tokens));
            return rows;
        }
        let Some(picker) = &settings.picker else {
            return Vec::new();
        };
        let mut rows = vec![
            row("Use this folder", Command::UseFolder).detail(folder_label(&picker.directory)),
        ];
        if let Some(parent) = picker.directory.parent() {
            rows.push(row("Up one folder", Command::Browse(parent.to_owned())));
        }
        rows.extend(picker.children.iter().map(|child| {
            let name = child.file_name().unwrap_or_default().to_string_lossy();
            let mut item = Item::new(
                display_filename(&name),
                Action::ExportSettings(origin, Command::Browse(child.clone())),
            )
            .detail(language.export_settings_text("Folder"));
            item.folder = true;
            item
        }));
        if picker.truncated {
            rows.push(Item::new(
                language.export_settings_text("Some folders are unavailable"),
                Action::Heading,
            ));
        }
        rows
    }

    pub(super) fn export_settings_command(
        &mut self,
        origin: Origin,
        command: Command,
    ) -> Result<()> {
        let mut preferences = self.export_settings.preferences.clone();
        match command {
            Command::Open => {
                self.show(Screen::ExportSettings(origin, Page::Settings));
                return Ok(());
            }
            Command::EditTemplate(scope) => {
                self.show(Screen::ExportSettings(origin, Page::Template(scope)));
                self.editor = Editor::new(filename::MAX_TEMPLATE_CHARS);
                if let Some(template) = preferences.template(scope) {
                    self.editor.text = template.chars().collect();
                    self.editor.caret = self.editor.text.len();
                }
                return Ok(());
            }
            Command::Tokens => {
                self.show(Screen::ExportSettings(origin, Page::Tokens));
                return Ok(());
            }
            Command::Browse(directory) => {
                let (directory, listing) =
                    with_storage(|| match self.library.browse(&directory) {
                        Ok(listing) => Ok((directory, listing)),
                        Err(_)
                            if !matches!(self.screen, Screen::ExportSettings(_, Page::Folders)) =>
                        {
                            Ok((PathBuf::new(), self.library.browse(Path::new(""))?))
                        }
                        Err(error) => Err(error),
                    })?;
                let children = listing
                    .entries
                    .into_iter()
                    .filter(|entry| matches!(entry.kind, BrowseKind::Folder))
                    .map(|entry| entry.path)
                    .collect();
                self.show(Screen::ExportSettings(origin, Page::Folders));
                self.export_settings.picker = Some(FolderPicker {
                    directory,
                    children,
                    truncated: listing.truncated,
                });
                return Ok(());
            }
            Command::UseFolder => {
                let picker = self
                    .export_settings
                    .picker
                    .as_ref()
                    .ok_or("Choose an export folder")?;
                with_storage(|| self.library.browse_folder(&picker.directory))?;
                if origin == Origin::Exchange {
                    return self.set_annotation_exchange_folder(Some(picker.directory.clone()));
                }
                preferences.folder = Some(picker.directory.clone());
            }
            Command::UseDefault => preferences.folder = None,
            Command::ToggleBookFolder => preferences.book_folder = !preferences.book_folder,
            Command::ToggleBookName => preferences.book_title = !preferences.book_title,
        }
        self.export_settings.save(preferences)?;
        self.show(Screen::ExportSettings(origin, Page::Settings));
        Ok(())
    }

    pub(super) fn back_from_export_settings(&mut self, origin: Origin, page: Page) {
        self.show(match (origin, page) {
            (Origin::Exchange, _) => Screen::AnnotationExchange,
            (_, Page::Folders | Page::Template(_) | Page::Tokens) => {
                Screen::ExportSettings(origin, Page::Settings)
            }
            (Origin::Book, Page::Settings) => Screen::ExportAnnotations,
            (Origin::Library, Page::Settings) => {
                Screen::LibraryExport(library_export::Page::Options)
            }
        });
    }

    pub(super) fn draw_export_template(&self, canvas: &mut Canvas, scope: Scope) {
        canvas.single_line_field(
            Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 130),
            &self.editor,
            scope.hint(),
            false,
        );
        canvas.text(
            Rect::new(LIST_X, LIST_Y + 148, WIDTH as i32 - 2 * LIST_X, 74),
            self.notice
                .as_deref()
                .unwrap_or("Save an empty field to use the default name."),
            CAPTION,
        );
        canvas.button(CLEAR_DRAFT, "Clear", false, false);
        canvas.keyboard(self.editor.layer, "Save", None);
        if self.can_paste_text() {
            external_link::draw_paste(canvas);
        }
    }

    pub(super) fn export_template_input(
        &mut self,
        origin: Origin,
        scope: Scope,
        input: Input,
    ) -> Change {
        if input == Input::PageBack
            || matches!(input, Input::Tap(x, y) if primitive_ui::back_button(WIDTH as i32).contains(x, y))
        {
            self.back_from_export_settings(origin, Page::Template(scope));
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if CLEAR_DRAFT.contains(x, y) {
            self.editor.text.clear();
            self.editor.caret = 0;
            self.notice = None;
            return Change::Partial;
        }
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Save", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let text = self.editor.string();
            if !filename::valid_template(&text) {
                return self.report("Enter up to 120 characters on one line");
            }
            let mut preferences = self.export_settings.preferences.clone();
            preferences.store_template(scope, text);
            if let Err(error) = self.export_settings.save(preferences) {
                return self.report(error);
            }
            self.back_from_export_settings(origin, Page::Template(scope));
        }
        Change::Partial
    }

    pub(super) fn write_annotation_export(
        &self,
        preferences: &Preferences,
        name: FilenameContext<'_>,
        extension: &str,
        contents: &[u8],
    ) -> io::Result<String> {
        let scope = if name.source.is_some() {
            Scope::Single
        } else {
            Scope::Multiple
        };
        let template = preferences.template(scope);
        let legacy_book = name.source.filter(|_| !name.bundled);
        let titled = legacy_book.is_some() && preferences.book_title;
        let fallback = || {
            legacy_book
                .and_then(|book| {
                    if titled && let Some(title) = safe_stem(book.title) {
                        return Some(title);
                    }
                    book.id.map(|id| format!("{id:016x}"))
                })
                .unwrap_or_else(|| "library-highlights".into())
        };
        let stem = template
            .and_then(|template| {
                let values;
                let context =
                    if let Some(last) = name.last_opened.filter(|_| scope == Scope::Multiple) {
                        last
                    } else {
                        values = self.filename_values(template, name.source, name.started_at);
                        &values
                    };
                safe_stem(&filename::expand(template, context))
            })
            .unwrap_or_else(fallback);
        with_storage(|| {
            let source = name
                .source
                .and_then(|book| book.file)
                .filter(|_| preferences.book_folder);
            let book_folder = if let Some(source) = source {
                let book = self.library.resolve(source)?;
                Some(
                    book.library_path()
                        .parent()
                        .unwrap_or_else(|| Path::new(""))
                        .to_owned(),
                )
            } else {
                None
            };
            let folder = book_folder.as_deref().or(preferences.folder.as_deref());
            let directory = if let Some(folder) = folder {
                self.library.browse_folder(folder)?
            } else {
                fs::create_dir_all(&self.exports)?;
                self.exports.clone()
            };
            let filename = if folder.is_some() || titled || template.is_some() {
                save_keep_both(&directory, &stem, extension, contents)?
            } else {
                let filename = format!("{stem}.{extension}");
                primitive_app::save(&directory.join(&filename), contents)?;
                filename
            };
            Ok(match folder {
                Some(folder) => format!("Documents/{}", folder.join(&filename).display()),
                None => format!("Reader data/exports/{filename}"),
            })
        })
    }
}

fn safe_stem(title: &str) -> Option<String> {
    let mut name = String::new();
    for ch in title.trim().chars() {
        let ch = if ch.is_whitespace() {
            ' '
        } else if ch.is_control() || r#"<>:"/\|?*"#.contains(ch) {
            '-'
        } else {
            ch
        };
        if name.len() + ch.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        name.push(ch);
    }
    let name = name.trim_matches([' ', '.', '-']);
    if name.is_empty() {
        return None;
    }
    let first = name.split('.').next()?.trim_end().to_ascii_uppercase();
    let device = matches!(first.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            first.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    Some(if device {
        format!("_{name}")
    } else {
        name.to_owned()
    })
}

fn save_keep_both(
    directory: &Path,
    stem: &str,
    extension: &str,
    contents: &[u8],
) -> io::Result<String> {
    for index in 1..=MAX_COPIES {
        let filename = if index == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{index}.{extension}")
        };
        let path = directory.join(&filename);
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
            if let (Ok(ours), Ok(current)) = (file.metadata(), fs::symlink_metadata(&path))
                && (ours.dev(), ours.ino(), ours.ctime(), ours.ctime_nsec())
                    == (
                        current.dev(),
                        current.ino(),
                        current.ctime(),
                        current.ctime_nsec(),
                    )
            {
                let _ = fs::remove_file(&path);
            }
            return Err(error);
        }
        return Ok(filename);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Export folder already contains too many copies",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_annotation_export_titles_are_single_vfat_safe_utf8_components() {
        assert_eq!(safe_stem(" ../read: \t紙\\*? "), Some("read-  紙".into()));
        assert_eq!(safe_stem("con.txt"), Some("_con.txt".into()));
        assert_eq!(safe_stem("COM¹.notes"), Some("_COM¹.notes".into()));
        assert_eq!(safe_stem("... / ?"), None);
        let long = safe_stem(&"界".repeat(100)).unwrap();
        assert_eq!(long.len(), 198);
        assert_eq!(long.chars().count(), 66);
    }
}
