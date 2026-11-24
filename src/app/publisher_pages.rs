//! Keep print-edition page labels attached to their publisher anchors as text reflows.
use super::{Action, Item, Reader, Screen};
use crate::Probe;
use primitive_app::WIDTH;
use primitive_ui::{Canvas, GrayImage, Rect, TextStyle};
use std::collections::HashMap;

pub(crate) struct PublisherPositions {
    ordered_and_resolved: bool,
    preceding: Option<usize>,
    chapters: Vec<Option<usize>>,
    screen_pages: HashMap<usize, usize>,
    // Each pair locates one print boundary in the open chapter's layout.
    visible: Vec<(f32, usize)>,
}

impl Probe {
    fn publisher_positions(
        &self,
        map: Option<&crate::synthetic_pages::CharacterPages>,
    ) -> &PublisherPositions {
        self.publisher_positions.get_or_init(|| {
            if let Some(map) = map {
                let mut chapters = Vec::with_capacity(map.count);
                let mut visible = Vec::new();
                let mut screen_pages = HashMap::new();
                let mut preceding = None;
                let mut ordered_and_resolved = map.book == self.book
                    && map.chapters.len() == self.chapter_entries().len()
                    && map.count > 0;
                for (chapter, (source, numbered)) in
                    self.chapter_entries().iter().zip(&map.chapters).enumerate()
                {
                    chapters.extend(std::iter::repeat_n(
                        Some(chapter + 1),
                        numbered.points.len(),
                    ));
                    if source.href != self.chapter {
                        continue;
                    }
                    preceding = numbered.first.checked_sub(1);
                    for (local, point) in numbered.points.iter().enumerate() {
                        let row = numbered.first + local;
                        let top = match point {
                            None => Some(0.0),
                            Some(p) => map
                                .anchor(chapter, *p, &source.href)
                                .and_then(|a| self.anchor_top(&a)),
                        };
                        if let Some(y) = top {
                            ordered_and_resolved &=
                                visible.last().is_none_or(|(prior, _)| *prior <= y);
                            visible.push((y, row));
                            screen_pages.insert(row, self.page_at(y) + 1);
                        } else {
                            ordered_and_resolved = false;
                        }
                    }
                }
                visible.sort_by(|(a, i), (b, j)| a.total_cmp(b).then(i.cmp(j)));
                return PublisherPositions {
                    ordered_and_resolved,
                    chapters,
                    screen_pages,
                    preceding,
                    visible,
                };
            }
            let order: HashMap<_, _> = self
                .chapter_entries()
                .iter()
                .enumerate()
                .map(|(i, chapter)| (chapter.href.as_str(), i))
                .collect();
            let chapter = order[self.chapter.as_str()];
            let mut preceding = None;
            let mut chapters = vec![None; self.printed_pages.len()];
            let mut screen_pages = HashMap::new();
            let mut visible = Vec::new();
            let mut ordered_and_resolved = !self.printed_pages.is_empty();
            let mut prior_section = None;
            for (i, entry) in self.printed_pages.iter().enumerate() {
                let Some(target) = &entry.destination else {
                    ordered_and_resolved = false;
                    continue;
                };
                let Some(&section) = order.get(target.path.as_str()) else {
                    ordered_and_resolved = false;
                    continue;
                };
                ordered_and_resolved &= prior_section.is_none_or(|prior| prior <= section);
                prior_section = Some(section);
                chapters[i] = Some(section + 1);
                if section < chapter {
                    preceding = preceding.max(Some((section, i)));
                } else if section == chapter {
                    let top = match &target.fragment {
                        Some(id) => self
                            .fragment(id)
                            .and_then(|anchor| self.anchor_top(&anchor)),
                        None => Some(0.0),
                    };
                    if let Some(top) = top {
                        ordered_and_resolved &=
                            visible.last().is_none_or(|(prior, _)| *prior <= top);
                        visible.push((top, i));
                        screen_pages.insert(i, self.page_at(top) + 1);
                    } else {
                        ordered_and_resolved = false;
                    }
                }
            }
            visible.sort_by(|(a, i), (b, j)| a.total_cmp(b).then(i.cmp(j)));
            PublisherPositions {
                ordered_and_resolved,
                chapters,
                screen_pages,
                preceding: preceding.map(|(_, i)| i),
                visible,
            }
        })
    }
}

impl Reader {
    pub(super) fn visible_publisher_page(&self) -> Option<usize> {
        let book = self.active.as_ref()?;
        let reading = &book.reading;
        let positions = reading.probe.publisher_positions(book.numbering.as_ref());
        let end = reading.visible_end();
        let next = positions.visible.partition_point(|(top, _)| *top < end);
        next.checked_sub(1)
            .map(|i| positions.visible[i].1)
            .or(positions.preceding)
    }

    pub(super) fn current_publisher_page(&self) -> Option<usize> {
        // CREngine labels even the front matter before the first print boundary.
        self.visible_publisher_page()
            .or_else(|| (self.active.as_ref()?.page_labels() > 0).then_some(0))
    }

    /// Count resolved reference pages spanned by a section in the current layout.
    pub(super) fn publisher_chapter_progress(
        &self,
        span: std::ops::Range<usize>,
    ) -> Option<(usize, usize)> {
        if !self.publisher_footer_enabled() {
            return None;
        }
        let book = self.active.as_ref()?;
        let reading = &book.reading;
        let probe = &reading.probe;
        let positions = probe.publisher_positions(book.numbering.as_ref());
        if reading.has_page_error() || !positions.ordered_and_resolved || span.is_empty() {
            return None;
        }
        let first_offset = probe.page_range(span.start).start;
        let next_offset = probe.page_range(span.end - 1).end;
        let row_before = |boundary, include_boundary| {
            let next = positions.visible.partition_point(|(top, _)| {
                *top < boundary || include_boundary && *top == boundary
            });
            next.checked_sub(1)
                .map(|index| positions.visible[index].1)
                .or(positions.preceding)
        };
        // An exact boundary belongs to the section after it. A print page
        // that spans the boundary is counted in both sections.
        let first = row_before(first_offset, true).unwrap_or(0);
        let last = row_before(next_offset, false)?;
        let current = self.current_publisher_page()?.clamp(first, last.max(first));
        Some((current - first + 1, last.checked_sub(first)? + 1))
    }

    pub(super) fn top_header_page_labels(&self) -> Option<(String, String)> {
        let book = self.active.as_ref()?;
        if book.numbering.is_none() && !self.publisher_footer_enabled() {
            return None;
        }
        let reading = &book.reading;
        let positions = reading.probe.publisher_positions(book.numbering.as_ref());
        if reading.has_page_error() || !positions.ordered_and_resolved {
            return None;
        }
        let current = book.page_label(self.current_publisher_page()?)?;
        // The final reference is either an authored label or part of the completed
        // character map. It does not describe the count of rendered screen pages.
        let last = book.page_label(book.page_labels().checked_sub(1)?)?;
        let current = compact_label(&current).trim();
        let last = compact_label(&last).trim();
        (!current.is_empty() && !last.is_empty()).then(|| (current.to_owned(), last.to_owned()))
    }

    pub(super) fn publisher_page_window(&self, first: usize, limit: usize) -> (usize, Vec<Item>) {
        let Some(book) = &self.active else {
            return (0, Vec::new());
        };
        let probe = &book.reading.probe;
        let geometry = probe.publisher_positions(book.numbering.as_ref());
        let count = book.page_labels()
            + usize::from(book.numbering.is_none() && probe.printed_source.is_some());
        let rows = (first..count.min(first.saturating_add(limit)))
            .filter_map(|row| {
                let item = if book.numbering.is_some() {
                    Item::new((row + 1).to_string(), Action::CharacterPage(row))
                } else if let Some(entry) = probe.printed_pages.get(row) {
                    Item::new(
                        &entry.label,
                        entry
                            .destination
                            .clone()
                            .map_or(Action::Heading, Action::Jump),
                    )
                } else {
                    return Some(
                        Item::new("Print edition source", Action::Heading)
                            .detail(probe.printed_source.as_ref()?),
                    );
                };
                Some(match geometry.chapters[row] {
                    Some(chapter) => item.detail(match geometry.screen_pages.get(&row) {
                        Some(page) => format!("Chapter {chapter} · Screen page {page}"),
                        None => format!("Chapter {chapter}"),
                    }),
                    None => item,
                })
            })
            .collect();
        (count, rows)
    }

    pub(super) fn publisher_footer_enabled(&self) -> bool {
        self.active
            .as_ref()
            .and_then(|book| self.library.state(book.id))
            .is_some_and(|state| state.publisher_page_footer)
    }

    pub(super) fn publisher_margins_enabled(&self) -> bool {
        self.active
            .as_ref()
            .and_then(|book| self.library.state(book.id))
            .is_some_and(|state| state.publisher_page_margins)
    }

    pub(super) fn toggle_publisher_labels(&mut self, margins: bool) -> crate::Result<()> {
        let book = self
            .active
            .as_ref()
            .filter(|book| book.page_labels() > 0)
            .ok_or("This book has no stable page labels")?;
        let id = book.id;
        let state = self
            .library
            .state_mut(id)
            .ok_or("Reading record unavailable")?;
        let field = if margins {
            &mut state.publisher_page_margins
        } else {
            &mut state.publisher_page_footer
        };
        let previous = *field;
        *field = !previous;
        self.mark_dirty();
        if let Err(error) = self.save_library() {
            let state = self.library.state_mut(id).unwrap();
            if margins {
                state.publisher_page_margins = previous;
            } else {
                state.publisher_page_footer = previous;
            }
            return Err(error.into());
        }
        self.show(Screen::Book);
        Ok(())
    }
    /// The compositor draws document pictures above ordinary UI text. Use a small reusable
    /// text surface so each label belongs to the page and keeps the page's night palette.
    pub(super) fn draw_publisher_margins(&self, canvas: &mut Canvas) {
        if !self.publisher_margins_enabled() {
            return;
        }
        let book = self.active.as_ref().unwrap();
        let reading = &book.reading;
        let probe = &reading.probe;
        let positions = &probe.publisher_positions(book.numbering.as_ref()).visible;
        if positions.is_empty() {
            return;
        }
        let paint = self.page_paint();
        const TILE_WIDTH: u32 = 144;
        let size = u32::from(self.controls.numbering.label_size.clamp(8, 20));
        // Keep the installed Reader’s default (14) at its original pixel height.
        let glyph_height = (18 * size + 7) / 14;
        let line = (25 * size + 7) / 14;
        if paint.visible.len() < line as usize {
            return;
        }
        let mut surface = self.publisher_label_canvas.borrow_mut();
        if surface.is_none() {
            let fonts = self
                .fonts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            *surface = Canvas::new(TILE_WIDTH, line, &fonts).ok();
        }
        let Some(tile) = surface.as_mut() else { return };
        for part in reading.visible_parts() {
            let range = part.range;
            let x = part.x;
            let left = reading.uses_facing_pages() && x == 0;
            let width = if left {
                probe.settings.left_margin()
            } else {
                probe.settings.right_margin()
            }
            .min(TILE_WIDTH);
            if width < 16 {
                continue;
            }
            let dest_x = if left {
                x
            } else {
                x + probe.settings.width - width
            };
            let mut minimum_y = paint.visible.start;
            let first = positions.partition_point(|(top, _)| *top < range.start);
            for &(top, row) in &positions[first..] {
                if top >= range.end {
                    break;
                }
                let y = part.y + (top - range.start + probe.settings.top_margin() as f32) as u32;
                if y < paint.visible.start || y >= paint.visible.end {
                    continue;
                }
                let y = y.min(paint.visible.end - line).max(minimum_y);
                if y + line > paint.visible.end {
                    break;
                }
                minimum_y = y + line;
                tile.begin();
                let Some(label) = book.page_label(row) else {
                    continue;
                };
                let label = compact_label(&label);
                let style = TextStyle::new((glyph_height * WIDTH / TILE_WIDTH) as i32);
                tile.text(
                    Rect::new(1, 0, width as i32 - 2, line as i32),
                    label,
                    if left { style } else { style.right() },
                );
                let pixels = tile
                    .finish(false)
                    .chunks_exact(TILE_WIDTH as usize)
                    .flat_map(|line| {
                        line[..width as usize]
                            .iter()
                            .map(|pixel| if paint.inverted { 255 - *pixel } else { *pixel })
                    })
                    .collect();
                canvas.picture(
                    Rect::new(dest_x as i32, y as i32, width as i32, line as i32),
                    GrayImage {
                        width,
                        height: line,
                        pixels,
                    },
                );
            }
        }
    }
}

pub(super) fn compact_label(label: &str) -> &str {
    if let Some(prefix) = label.get(..4)
        && prefix.eq_ignore_ascii_case("page")
    {
        let number = label[4..].trim_start();
        if !number.is_empty()
            && (number.len() < label.len() - 4 || number.starts_with(|c: char| c.is_ascii_digit()))
        {
            return number;
        }
    }
    label
}
