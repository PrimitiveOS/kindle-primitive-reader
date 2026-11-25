//! Verified quote checkpoints for one foreground EPUB session.
#[cfg(test)]
mod tests;

use std::{cell::RefCell, path::Path, rc::Rc};

use super::screen_pages::FileStamp;
use crate::{Result, book_style::pseudo::quotes::Checkpoints, epub::Book};

#[derive(Clone, Default)]
pub(crate) struct ForegroundQuotes(Rc<RefCell<Option<Saved>>>);

struct Saved {
    stamp: FileStamp,
    edition: u64,
    checkpoints: Box<Checkpoints>,
}

impl ForegroundQuotes {
    pub(super) fn prepare(&self, path: &Path, book: &mut Book) -> Result<Option<FileStamp>> {
        let saved = self.0.borrow_mut().take();
        let Some(stamp) = book.quote_file_stamp().cloned() else {
            return Ok(None);
        };
        verify_stamp(path, &stamp)?;
        book.quote_prefixes = Some(match saved {
            Some(saved) if saved.stamp == stamp && saved.edition == book.revision() => {
                saved.checkpoints
            }
            _ => Box::new(Checkpoints::foreground()),
        });
        Ok(Some(stamp))
    }

    pub(super) fn save(&self, path: &Path, book: &mut Book, stamp: FileStamp) -> Result<()> {
        verify_stamp(path, &stamp)?;
        if let Some(checkpoints) = book.quote_prefixes.take() {
            *self.0.borrow_mut() = Some(Saved {
                stamp,
                edition: book.revision(),
                checkpoints,
            });
        }
        Ok(())
    }

    pub(super) fn clear(&self) {
        *self.0.borrow_mut() = None;
    }

    #[cfg(test)]
    fn latest(&self) -> Option<usize> {
        self.0.borrow().as_ref()?.checkpoints.latest()
    }
}

fn verify_stamp(path: &Path, stamp: &FileStamp) -> Result<()> {
    if FileStamp::read(path).as_ref() == Some(stamp) {
        Ok(())
    } else {
        Err("The EPUB changed during chapter layout. Reopen the book.".into())
    }
}
