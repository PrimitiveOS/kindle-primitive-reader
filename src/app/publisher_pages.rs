//! Keep print-edition page labels attached to their publisher anchors as text reflows.
use super::{Action, Item, Reader, Screen};
use crate::Probe;
use primitive_app::WIDTH;
use primitive_ui::{Canvas, GrayImage, Rect, TextStyle};
use std::collections::HashMap;

pub(crate) struct PublisherPositions {
    preceding: Option<usize>,
    // Each pair locates one print boundary in the open chapter's layout.
    visible: Vec<(f32, usize)>,
}

impl Probe {
    fn publisher_positions(&self) -> &PublisherPositions {
        self.publisher_positions.get_or_init(|| {
            let order: HashMap<_, _> = self
                .chapter_entries()
                .iter()
                .enumerate()
                .map(|(i, chapter)| (chapter.href.as_str(), i))
                .collect();
            let chapter = order[self.chapter.as_str()];
            let mut preceding = None;
            let mut visible = Vec::new();
            for (i, entry) in self.printed_pages.iter().enumerate() {
                let Some(target) = &entry.destination else {
                    continue;
                };
                let Some(&section) = order.get(target.path.as_str()) else {
                    continue;
                };
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
                        visible.push((top, i));
                    }
                }
            }
            visible.sort_by(|(a, i), (b, j)| a.total_cmp(b).then(i.cmp(j)));
            PublisherPositions {
                preceding: preceding.map(|(_, i)| i),
                visible,
            }
        })
    }
}

impl Reader {
    pub(super) fn visible_publisher_page(&self) -> Option<usize> {
        let reading = &self.active.as_ref()?.reading;
        let positions = reading.probe.publisher_positions();
        let end = reading.visible_end();
        let next = positions.visible.partition_point(|(top, _)| *top < end);
        next.checked_sub(1)
            .map(|i| positions.visible[i].1)
            .or(positions.preceding)
    }

    pub(super) fn current_publisher_page(&self) -> Option<usize> {
        // CREngine labels even the front matter before the first print boundary.
        self.visible_publisher_page().or_else(|| {
            (!self.active.as_ref()?.reading.probe.printed_pages.is_empty()).then_some(0)
        })
    }

    pub(super) fn publisher_page_items(&self) -> Vec<Item> {
        let Some(book) = &self.active else {
            return Vec::new();
        };
        let probe = &book.reading.probe;
        let chapters: HashMap<_, _> = probe
            .chapter_entries()
            .iter()
            .enumerate()
            .map(|(i, chapter)| (chapter.href.as_str(), i + 1))
            .collect();
        let screen_pages: HashMap<_, _> = probe
            .publisher_positions()
            .visible
            .iter()
            .map(|(top, row)| (*row, probe.page_at(*top) + 1))
            .collect();
        let mut items: Vec<_> = probe
            .printed_pages
            .iter()
            .enumerate()
            .map(|(row, entry)| {
                let item = Item::new(
                    &entry.label,
                    entry
                        .destination
                        .clone()
                        .map_or(Action::Heading, Action::Jump),
                );
                let Some(chapter) = entry
                    .destination
                    .as_ref()
                    .and_then(|d| chapters.get(d.path.as_str()))
                else {
                    return item;
                };
                item.detail(match screen_pages.get(&row) {
                    Some(page) => format!("Chapter {chapter} · Screen page {page}"),
                    None => format!("Chapter {chapter}"),
                })
            })
            .collect();
        if let Some(source) = &probe.printed_source {
            items.push(Item::new("Print edition source", Action::Heading).detail(source));
        }
        items
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
            .filter(|book| !book.reading.probe.printed_pages.is_empty())
            .ok_or("This book has no publisher page labels")?;
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
        let reading = &self.active.as_ref().unwrap().reading;
        let probe = &reading.probe;
        let positions = &probe.publisher_positions().visible;
        if positions.is_empty() {
            return;
        }
        let paint = self.page_paint();
        const TILE_WIDTH: u32 = 144;
        const LINE: u32 = 25;
        let width = probe.settings.side_margin().min(TILE_WIDTH);
        if width < 16 || paint.visible.len() < LINE as usize {
            return;
        }
        let mut surface = self.publisher_label_canvas.borrow_mut();
        if surface.is_none() {
            let fonts = self
                .fonts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            *surface = Canvas::new(TILE_WIDTH, LINE, &fonts).ok();
        }
        let Some(tile) = surface.as_mut() else { return };
        for (range, x) in reading.visible_columns() {
            let left = reading.uses_facing_pages() && x == 0;
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
                let y = (top - range.start + probe.settings.top_margin() as f32) as u32;
                if y < paint.visible.start || y >= paint.visible.end {
                    continue;
                }
                let y = y.min(paint.visible.end - LINE).max(minimum_y);
                if y + LINE > paint.visible.end {
                    break;
                }
                minimum_y = y + LINE;
                tile.begin();
                let label = compact_label(&probe.printed_pages[row].label);
                let style = TextStyle::new((18 * WIDTH / TILE_WIDTH) as i32);
                tile.text(
                    Rect::new(1, 0, width as i32 - 2, LINE as i32),
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
                    Rect::new(dest_x as i32, y as i32, width as i32, LINE as i32),
                    GrayImage {
                        width,
                        height: LINE,
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
