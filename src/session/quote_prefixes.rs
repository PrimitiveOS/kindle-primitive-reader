//! Verified quote checkpoints for one foreground EPUB session.
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod worker;

use std::{cell::RefCell, path::Path, rc::Rc};

use super::screen_pages::FileStamp;
use crate::{
    Result,
    book_style::pseudo::quotes::{Checkpoints, QuoteNeed},
    epub::Book,
};

#[derive(Clone, Default)]
pub(crate) struct ForegroundQuotes(Rc<RefCell<Option<Saved>>>);

struct Saved {
    stamp: FileStamp,
    edition: u64,
    checkpoints: Box<Checkpoints>,
}

/// Owned by a committed Reading, never by a preview or speculative layout.
#[derive(Clone, PartialEq)]
pub(crate) struct Marker {
    pub(crate) stamp: FileStamp,
    pub(crate) edition: u64,
    pub(crate) need: QuoteNeed,
}

impl ForegroundQuotes {
    pub(crate) fn stamp(&self) -> Option<FileStamp> {
        self.0.borrow().as_ref().map(|saved| saved.stamp.clone())
    }

    pub(crate) fn need(&self) -> Option<QuoteNeed> {
        self.0.borrow().as_ref()?.checkpoints.need()
    }

    pub(super) fn marker(&self, edition: u64, chapter: usize) -> Option<Marker> {
        let slot = self.0.borrow();
        let saved = slot.as_ref()?;
        let need = saved.checkpoints.need()?;
        (saved.edition == edition && need.chapter == chapter).then(|| Marker {
            stamp: saved.stamp.clone(),
            edition,
            need,
        })
    }

    /// A USB return invalidates all checkpoints. Rearm only the committed
    /// marker if the same file identity and metadata are still present.
    pub(crate) fn rearm(&self, path: &Path, marker: &Marker) -> bool {
        let matches = primitive_app::with_storage(|| {
            Ok(FileStamp::read(path).as_ref() == Some(&marker.stamp))
        })
        .unwrap_or(false);
        if !matches {
            self.clear();
            return false;
        }
        let mut checkpoints = Checkpoints::foreground();
        checkpoints.set_need(marker.need.clone());
        *self.0.borrow_mut() = Some(Saved {
            stamp: marker.stamp.clone(),
            edition: marker.edition,
            checkpoints: Box::new(checkpoints),
        });
        true
    }

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
