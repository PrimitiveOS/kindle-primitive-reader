//! Apply ordinary-tap policies at the reading surface. Explicit link rows
//! and links inside a footnote preview use their own direct actions.
use super::*;
use crate::fixed::PdfLink;
use crate::selection::HighlightRect;
use std::collections::BTreeSet;

// KOReader's 30 px radius at 167 DPI on Reader's 300 DPI logical canvas.
const LINK_TAP_RADIUS: f32 = 30.0 * 300.0 / 167.0;

#[derive(Clone, Copy)]
pub(super) enum LinkPreference {
    Follow,
    LargerArea,
    IgnoreExternal,
}

impl LinkPreference {
    pub(super) fn checked(self, controls: ReadingControls) -> bool {
        match self {
            Self::Follow => controls.tap_links.unwrap_or(true),
            Self::LargerArea => controls.larger_tap_area_for_links,
            Self::IgnoreExternal => controls.ignore_external_link_taps,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Follow => "Tap to follow links",
            Self::LargerArea => "Allow larger tap area around links",
            Self::IgnoreExternal => "Ignore external links on tap",
        }
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum ReflowLink {
    Internal(Destination),
    Address(String),
}

impl ReflowLink {
    fn resolve(chapter: &str, href: &str) -> Option<Self> {
        if let Ok(target) = Book::resolve_link(chapter, href) {
            return Some(Self::Internal(target));
        }
        let address = url::Url::parse(href).ok()?;
        (matches!(address.scheme(), "http" | "https")
            && address.has_host()
            && !href.chars().any(char::is_control)
            && href.len() <= 4096)
            .then(|| Self::Address(href.to_owned()))
    }
}

impl Reader {
    pub(super) fn link_tap_settings_items(&self) -> Vec<Item> {
        [
            LinkPreference::Follow,
            LinkPreference::LargerArea,
            LinkPreference::IgnoreExternal,
        ]
        .into_iter()
        .filter(|setting| !matches!(setting, LinkPreference::LargerArea) || self.active.is_some())
        .map(|setting| {
            let enabled = matches!(setting, LinkPreference::Follow)
                || LinkPreference::Follow.checked(self.controls);
            let item = Item::new(
                setting.label(),
                if enabled {
                    Action::ToggleLinkPreference(setting)
                } else {
                    Action::Heading
                },
            );
            if enabled {
                item
            } else {
                item.detail("Available when tap to follow links is on")
            }
        })
        .collect()
    }

    pub(super) fn toggle_link_preference(&mut self, setting: LinkPreference) -> Result<()> {
        if !matches!(setting, LinkPreference::Follow)
            && !LinkPreference::Follow.checked(self.controls)
        {
            return Ok(());
        }
        let checked = setting.checked(self.controls);
        let mut controls = self.controls;
        match setting {
            LinkPreference::Follow => controls.tap_links = Some(!checked),
            LinkPreference::LargerArea => controls.larger_tap_area_for_links = !checked,
            LinkPreference::IgnoreExternal => controls.ignore_external_link_taps = !checked,
        }
        self.activate(Action::SetReadingControls(controls))
    }

    pub(super) fn reflow_tap_link(&self, x: f32, y: f32) -> Option<(ReflowLink, f32, f32)> {
        if !LinkPreference::Follow.checked(self.controls) {
            return None;
        }
        let reading = &self.active.as_ref()?.reading;
        if let Some(link) = reading
            .link_href_at(x, y)
            .and_then(|href| ReflowLink::resolve(&reading.probe.chapter, href))
            .filter(|link| {
                !self.controls.ignore_external_link_taps || matches!(link, ReflowLink::Internal(_))
            })
        {
            return Some((link, x, y));
        }
        if !self.controls.larger_tap_area_for_links || !(0.0..PAGE_HEIGHT as f32).contains(&y) {
            return None;
        }
        nearest_internal_link(reading, x, y)
            .map(|(target, x, y)| (ReflowLink::Internal(target), x, y))
    }

    pub(super) fn fixed_tap_link(&self, x: f32, y: f32) -> Option<PdfLink> {
        if !LinkPreference::Follow.checked(self.controls) {
            return None;
        }
        self.fixed.as_ref()?.reading.link_at(x, y).filter(|link| {
            !self.controls.ignore_external_link_taps || link.target != PdfLinkTarget::External
        })
    }

    pub(super) fn open_fixed_link(&mut self, link: PdfLink) -> Result<()> {
        match link.target {
            PdfLinkTarget::Page(page) => {
                self.follow_fixed(page, link.destination_point.map(FixedJump::LinkPoint))
            }
            PdfLinkTarget::External => {
                let address = link
                    .uri
                    .ok_or("This link refers to a resource outside the PDF")?;
                self.show(self.surface_screen());
                self.show_link_address(address);
                Ok(())
            }
            PdfLinkTarget::Unavailable => Err("This PDF link is unavailable".into()),
        }
    }

    pub(super) fn page_link_items(&self) -> Vec<Item> {
        if let Some(fixed) = &self.fixed {
            return fixed
                .reading
                .visible_links()
                .into_iter()
                .filter_map(|link| match link.target {
                    PdfLinkTarget::Page(page) => Some(Item::new(
                        fixed_page_label(&fixed.reading, page),
                        Action::OpenFixedLink(link),
                    )),
                    PdfLinkTarget::External => {
                        let address = link.uri.clone()?;
                        Some(
                            Item::new("External link", Action::OpenFixedLink(link)).detail(address),
                        )
                    }
                    PdfLinkTarget::Unavailable => None,
                })
                .collect();
        }
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let reading = &active.reading;
        let mut links = Vec::new();
        for (range, _) in reading.visible_columns() {
            for (label, href) in reading.probe.link_hrefs_in(range) {
                if let Some(target) = ReflowLink::resolve(&reading.probe.chapter, href)
                    && !links
                        .iter()
                        .any(|(existing, link)| existing == &label && link == &target)
                {
                    links.push((label, target));
                }
            }
        }
        links
            .into_iter()
            .map(|(label, target)| match target {
                ReflowLink::Internal(target) => Item::new(label, Action::Jump(target)),
                ReflowLink::Address(address) => {
                    Item::new(label, Action::OpenLinkAddress(address.clone())).detail(address)
                }
            })
            .collect()
    }
}

fn nearest_internal_link(reading: &Reading, x: f32, y: f32) -> Option<(Destination, f32, f32)> {
    let probe = &reading.probe;
    let mut nearest = None;
    let mut distance = LINK_TAP_RADIUS * LINK_TAP_RADIUS;
    for part in reading.visible_parts() {
        let local_x = x - part.x as f32;
        let local_y = y - part.y as f32;
        let content_y = local_y - probe.settings.top_margin() as f32 + part.range.start;
        let top = part.range.start.max(content_y - LINK_TAP_RADIUS);
        let end = part.range.end.min(content_y + LINK_TAP_RADIUS);
        if top >= end {
            continue;
        }
        let mut consider = |rect: HighlightRect, target: &Destination| {
            let x0 = rect.x as f32;
            let y0 = rect.y as f32;
            let x1 = (rect.x + rect.width) as f32;
            let y1 = (rect.y + rect.height) as f32;
            let dx = local_x - local_x.clamp(x0, x1);
            let dy = local_y - local_y.clamp(y0, y1);
            let candidate = dx * dx + dy * dy;
            if candidate <= distance && (candidate < distance || nearest.is_none()) {
                distance = candidate;
                nearest = Some((
                    target.clone(),
                    part.x as f32 + (x0 + x1) * 0.5,
                    part.y as f32 + (y0 + y1) * 0.5,
                ));
            }
        };
        let first = probe
            .lines
            .partition_point(|line| line.top < top - probe.settings.text_height() as f32);
        let last = probe.lines.partition_point(|line| line.top < end);
        let blocks: BTreeSet<_> = probe.lines[first..last]
            .iter()
            .filter(|line| line.bottom > top)
            .map(|line| line.block)
            .collect();
        for key in blocks {
            let Ok(index) = probe
                .source
                .blocks
                .binary_search_by_key(&key, |block| block.key)
            else {
                continue;
            };
            for link in &probe.source.blocks[index].links {
                let Ok(target) = Book::resolve_link(&probe.chapter, &link.href) else {
                    continue;
                };
                let Some(selection) = probe.select_range(
                    &probe.anchor(key, link.bytes.start),
                    &probe.anchor(key, link.bytes.end),
                ) else {
                    continue;
                };
                for rect in probe.selection_rects_in(part.range.clone(), &selection) {
                    consider(rect, &target);
                }
            }
        }
        if let crate::Renderer::Parley(layout) = &probe.renderer {
            for (href, cols, rows) in layout.image_link_regions(part.range.clone(), probe.settings)
            {
                if let Ok(target) = Book::resolve_link(&probe.chapter, href) {
                    consider(
                        HighlightRect {
                            x: cols.start as i32,
                            y: rows.start as i32,
                            width: cols.len() as i32,
                            height: rows.len() as i32,
                        },
                        &target,
                    );
                }
            }
        }
    }
    nearest
}

#[cfg(test)]
mod tests;
