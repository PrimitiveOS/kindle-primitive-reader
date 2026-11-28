//! The Reader-wide fallback font. Characters that a book's own font lacks are drawn in this
//! font. It follows the book's font in the list (KOReader's `fallback_font`).

use std::fs;
use std::io;
use std::path::Path;

use primitive_app::with_storage;

use crate::reading_fonts;

const MAX_BYTES: u64 = 512;

pub(crate) fn load(path: &Path, fonts_directory: &Path) -> Option<String> {
    with_storage(|| {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid fallback font preference",
            ));
        }
        let value: Option<String> =
            serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        Ok(value)
    })
    .ok()
    .flatten()
    .filter(|name| reading_fonts::resolve(fonts_directory, name).is_ok())
}

pub(crate) fn save(path: &Path, fonts_directory: &Path, file: Option<&str>) -> io::Result<()> {
    if let Some(file) = file {
        reading_fonts::resolve(fonts_directory, file)?;
    }
    let bytes = serde_json::to_vec(&file).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Fallback font preference is too long",
        ));
    }
    primitive_app::save(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Fixture;
    use crate::app::{Action, Reader};
    use primitive_app::App;

    fn canonical(path: &Path) -> std::path::PathBuf {
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn fallback_font_follows_the_book_font_survives_reopening_and_turns_off() {
        let fixture = Fixture::new();
        let fonts_dir = fixture.root.join("fonts");
        fs::create_dir_all(&fonts_dir).unwrap();
        fs::copy(
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            fonts_dir.join("NotoSans-Bold.ttf"),
        )
        .unwrap();
        fs::copy(
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            fonts_dir.join("NotoSans-Regular.ttf"),
        )
        .unwrap();
        let bold = fs::canonicalize(fonts_dir.join("NotoSans-Bold.ttf")).unwrap();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert!(!app.fonts_for(None).unwrap().iter().any(|path| path.ends_with("NotoSans-Bold.ttf")));

        app.activate(Action::SetFallbackFont(Some("NotoSans-Bold.ttf".into())))
            .unwrap();
        let fonts = app.fonts_for(None).unwrap();
        assert_eq!(canonical(&fonts[1]), bold, "the fallback comes right after the primary font");

        let fonts = app.fonts_for(Some("NotoSans-Regular.ttf")).unwrap();
        assert!(fonts[0].ends_with("NotoSans-Regular.ttf"));
        assert_eq!(canonical(&fonts[1]), bold);

        app.save().unwrap();
        let fonts = vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(canonical(&reopened.fonts_for(None).unwrap()[1]), bold);

        reopened.activate(Action::SetFallbackFont(None)).unwrap();
        assert!(!reopened
            .fonts_for(None)
            .unwrap()
            .iter()
            .any(|path| path.ends_with("NotoSans-Bold.ttf")));
    }

    #[test]
    fn a_missing_fallback_file_is_refused_and_never_saved() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.open_path(&fixture.book).unwrap();
        assert!(app.activate(Action::SetFallbackFont(Some("Missing.ttf".into()))).is_err());
        assert!(fixture.root.join("fonts").join("Missing.ttf").exists() == false);
        assert_eq!(app.fallback_font_file, None);
    }
}
