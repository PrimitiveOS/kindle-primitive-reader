//! Hold a prepared reading view until the runtime verifies its presentation.

use super::{Reader, ReadingPosition};
use crate::Result;
use crate::fixed_session::PreparedFixedViewport;
use crate::session::{Reading, ReadingViewport};
use primitive_app::App;
use primitive_ui::Canvas;

enum Candidate {
    Current,
    Reflow(Box<Reading>),
    Fixed(Box<PreparedFixedViewport>),
}

pub(super) struct Stage {
    book: Option<u64>,
    position: Option<ReadingPosition>,
    from: ReadingViewport,
    target: ReadingViewport,
    candidate: Candidate,
    marks: Option<(super::saved_marks_view::FlexibleMarks, usize)>,
}

impl Reader {
    pub(super) fn prepare_reading_viewport(&self, target: ReadingViewport) -> Result<Stage> {
        let from = self.reading_viewport();
        let candidate = if target == from {
            Candidate::Current
        } else if let Some(book) = &self.active {
            Candidate::Reflow(Box::new(book.reading.prepare_viewport(target)?))
        } else if let Some(book) = &self.fixed {
            Candidate::Fixed(Box::new(book.reading.prepare_viewport(target)?))
        } else {
            return Err("Open a document before changing its viewport".into());
        };
        Ok(Stage {
            book: self.open_book_id(),
            position: self.current_position(),
            from,
            target,
            candidate,
            marks: None,
        })
    }

    pub(super) fn draw_prepared_reading_viewport(
        &mut self,
        staged: &mut Stage,
        canvas: &mut Canvas,
    ) -> Result<()> {
        staged.check_origin(self)?;
        let (width, height) = staged.target.dimensions();
        if canvas.bounds().w != width as i32 || canvas.bounds().h != height as i32 {
            return Err("The staged canvas has different reading dimensions".into());
        }
        self.exchange_staged_reading(&mut staged.candidate)?;
        let drawn = self.draw_staged_content(staged, canvas);
        let restored = self.exchange_staged_reading(&mut staged.candidate);
        drawn.and(restored)
    }

    fn draw_staged_content(&mut self, staged: &mut Stage, canvas: &mut Canvas) -> Result<()> {
        if staged.from != staged.target
            && let Some((marks, page)) = self.resize_flexible_marks()?
        {
            let previous_marks = self.flexible_marks.replace(marks);
            let previous_page = std::mem::replace(&mut self.list_page, page);
            self.draw(canvas);
            staged.marks = self.flexible_marks.take().map(|view| (view, page));
            self.flexible_marks = previous_marks;
            self.list_page = previous_page;
        } else {
            self.draw(canvas);
        }
        Ok(())
    }

    pub(super) fn commit_reading_viewport(&mut self, staged: Stage) -> Result<()> {
        staged.check_origin(self)?;
        match staged.candidate {
            Candidate::Current => return Ok(()),
            Candidate::Reflow(reading) => {
                self.active.as_mut().ok_or("The book closed")?.reading = *reading;
            }
            Candidate::Fixed(prepared) => {
                self.fixed
                    .as_mut()
                    .ok_or("The document closed")?
                    .reading
                    .commit_viewport(*prepared)?;
            }
        }
        if let Some((marks, page)) = staged.marks {
            self.flexible_marks = Some(marks);
            self.list_page = page;
        }
        self.screen_page_worker.cancel();
        self.pages_since_refresh = 0;
        self.record_position();
        Ok(())
    }

    fn exchange_staged_reading(&mut self, candidate: &mut Candidate) -> Result<()> {
        match candidate {
            Candidate::Current => Ok(()),
            Candidate::Reflow(reading) => {
                let active = self.active.as_mut().ok_or("The book closed")?;
                std::mem::swap(&mut active.reading, reading.as_mut());
                Ok(())
            }
            Candidate::Fixed(prepared) => self
                .fixed
                .as_mut()
                .ok_or("The document closed")?
                .reading
                .exchange_prepared_viewport(prepared),
        }
    }
}

impl Stage {
    fn check_origin(&self, reader: &Reader) -> Result<()> {
        if reader.open_book_id() != self.book
            || reader.current_position() != self.position
            || reader.reading_viewport() != self.from
        {
            return Err("The document changed before the viewport was presented".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Fixture;
    use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision};

    #[test]
    fn viewport_hooks_prepare_and_draw_before_adopting_reflow_and_fixed_frames() {
        for fixed in [false, true] {
            let fixture = Fixture::new();
            let pdf = fixture.root.join("Tilted Scan.pdf");
            std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
            let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
            let mut reader = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
            reader
                .open_path(if fixed { &pdf } else { &fixture.book })
                .unwrap();
            let original = reader.current_history_location().unwrap();
            let original_dimensions = reader.reading_viewport();
            let paths: Vec<_> = fonts
                .iter()
                .map(|font| font.display().to_string())
                .collect();
            let target = Viewport {
                generation: 2,
                turn: LogicalTurn::Clockwise,
                logical_size: ReadingViewport::LANDSCAPE.dimensions(),
            };
            let (width, height) = target.logical_size;
            let mut staged = Canvas::new_viewport(width, height, 1264, &paths).unwrap();
            reader.stage_viewport(target).unwrap();
            staged.begin();
            reader.draw_staged_viewport(&mut staged).unwrap();
            assert_eq!(reader.current_history_location().as_ref(), Some(&original));
            assert_eq!(reader.reading_viewport(), original_dimensions);
            let frame = staged.finish(false);
            assert_eq!(frame.len(), (width * height) as usize);
            assert!(frame.iter().any(|ink| *ink < 64));
            reader.finish_viewport(target, ViewportDecision::Abort);
            assert_eq!(reader.reading_viewport(), original_dimensions);
            assert!(reader.staged_viewport.is_none());

            let target = Viewport {
                generation: 3,
                ..target
            };
            reader.stage_viewport(target).unwrap();
            let mut wrong = Canvas::new(1264, 1680, &paths).unwrap();
            assert!(reader.draw_staged_viewport(&mut wrong).is_err());
            assert_eq!(reader.reading_viewport(), original_dimensions);
            staged.begin();
            reader.draw_staged_viewport(&mut staged).unwrap();
            reader.finish_viewport(target, ViewportDecision::Commit);
            assert_eq!(reader.reading_viewport(), ReadingViewport::LANDSCAPE);
            assert_eq!(
                reader.current_position().unwrap().chapter,
                original.position.chapter
            );
            assert!(reader.staged_viewport.is_none());
        }
    }
}
