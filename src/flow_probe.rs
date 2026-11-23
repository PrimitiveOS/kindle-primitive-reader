//! Publisher presentation intent for verification. These classifications never
//! assert that geometry succeeded: an eligible point without layout remains an
//! unexpected failure, including visibility-hidden content.
use serde::Serialize;

use crate::{Anchor, Probe, flow_projection::Affinity};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePresentation {
    Normal,
    VisibilityHidden,
    /// Canonical bytes intentionally removed/re-collapsed by the CSS view.
    CssOmitted,
}

impl Probe {
    /// Readable layout fallbacks are reported separately from full fidelity.
    pub fn layout_limitations(&self) -> &[String] {
        match &self.renderer {
            crate::Renderer::Parley(renderer) => renderer.limitations(),
            _ => &[],
        }
    }
    /// Resolve a canonical/legacy anchor's presentation intent without changing
    /// its stored location. None means the source anchor itself is unavailable.
    pub fn source_presentation(&self, anchor: &Anchor) -> Option<SourcePresentation> {
        let (index, offset) = self.project_anchor(anchor)?;
        let block = &self.source.blocks[index];
        if let Some(ordinal) = anchor
            .illustration
            .or_else(|| block.text.is_empty().then_some(0))
            .filter(|_| !block.images.is_empty())
        {
            if offset != 0 {
                return None;
            }
            let image = block.images.get(ordinal)?;
            let flow = self.reading_flow.element(image.element).ok()?;
            return Some(if flow.suppressed {
                SourcePresentation::CssOmitted
            } else if !flow.painted {
                SourcePresentation::VisibilityHidden
            } else {
                SourcePresentation::Normal
            });
        }
        let view = self.reading_flow.view(&self.source, block).ok()?;
        if offset < block.text.len() && !view.source_is_present(offset)
            || view.project_offset(offset, Affinity::Before).is_none()
                && view.project_offset(offset, Affinity::After).is_none()
        {
            return Some(SourcePresentation::CssOmitted);
        }
        let painted = match view.runs() {
            Some(runs) => runs
                .iter()
                .find(|run| run.source.contains(&offset))
                .or_else(|| runs.iter().rev().find(|run| run.source.end == offset))
                .map(|run| run.painted)?,
            None => {
                let element = block
                    .styled_text
                    .iter()
                    .find(|run| run.bytes.contains(&offset))
                    .or_else(|| {
                        block
                            .styled_text
                            .iter()
                            .rev()
                            .find(|run| run.bytes.end == offset)
                    })
                    .map_or(block.element, |run| run.element);
                self.reading_flow.element(element).ok()?.painted
            }
        };
        Some(if painted {
            SourcePresentation::Normal
        } else {
            SourcePresentation::VisibilityHidden
        })
    }

    /// Empty/synthetic IDs retain their true element owner even when Source's
    /// canonical position is at a following paragraph. Never reveal or retarget
    /// a hidden owner simply because that old caret position happens to exist.
    pub fn fragment_presentation(&self, name: &str) -> Option<SourcePresentation> {
        let owner = self.source.fragment_owner(name)?;
        let flow = self.reading_flow.element(owner).ok()?;
        Some(if flow.suppressed {
            SourcePresentation::CssOmitted
        } else if !flow.painted {
            SourcePresentation::VisibilityHidden
        } else {
            SourcePresentation::Normal
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Book, Engine, Settings};

    fn open(markup: &str, publisher_styles: bool) -> Probe {
        Probe::from_book_content(
            Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                width: 600,
                height: 800,
                margin: 36,
                font_size: 24.,
                publisher_styles,
                ..Default::default()
            },
            0,
            Some(markup.into()),
        )
        .unwrap()
    }

    fn screenshot(name: &str, probe: &mut Probe) {
        if let Some(root) = std::env::var_os("READER_FLOW_SCREENS") {
            image::save_buffer(
                std::path::PathBuf::from(root).join(format!("{name}.png")),
                &probe.capture(0).unwrap(),
                probe.settings.width,
                probe.settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }

    #[test]
    fn display_projection_preserves_source_marks_and_resolves_visible_container_prefix() {
        let html = "<html><body><p id='target'><span id='removed' style='display:none'>SECRET</span>Visible <span style='display:none'>TRAP</span>words.</p><p id='after'>After.</p></body></html>";
        let mut plain = open(html, false);
        let mut styled = open(html, true);
        screenshot("display-canonical", &mut plain);
        screenshot("display-projected", &mut styled);
        assert_eq!(plain.source.revision, styled.source.revision);
        assert_eq!(plain.source.fragments, styled.source.fragments);
        assert_eq!(
            plain
                .source
                .blocks
                .iter()
                .map(|b| &b.text)
                .collect::<Vec<_>>(),
            styled
                .source
                .blocks
                .iter()
                .map(|b| &b.text)
                .collect::<Vec<_>>()
        );
        assert!(styled.matches("SECRET").is_empty());
        assert!(styled.matches("TRAP").is_empty());
        assert_eq!(
            styled.fragment_presentation("removed"),
            Some(SourcePresentation::CssOmitted)
        );
        assert!(styled.fragment("removed").is_none());
        assert_eq!(
            styled.fragment_presentation("target"),
            Some(SourcePresentation::Normal)
        );
        let target = styled.fragment("target").unwrap();
        assert_eq!(target, styled.matches("Visible")[0]);
        assert!(styled.page_for(&target).is_some());
        let old = plain.fragment("target").unwrap();
        assert_eq!(
            styled.source_presentation(&old),
            Some(SourcePresentation::CssOmitted)
        );
        assert!(styled.page_for(&old).is_none());
        let end = crate::Anchor {
            offset: plain.source.blocks[0].text.len(),
            ..old.clone()
        };
        let canonical = plain.select_range(&old, &end).unwrap();
        assert!(canonical.text.contains("SECRET") && canonical.text.contains("TRAP"));
        assert_eq!(
            styled.select_source_range(&old, &end).unwrap().text,
            canonical.text
        );
        let selected = styled.select_range(&old, &end).unwrap();
        assert_eq!(selected.text, "Visible words.");
        assert!(!styled.selection_rects(0, &canonical).is_empty());
        let (mapped, total) = styled.coverage();
        let (plain_mapped, plain_total) = plain.coverage();
        assert_eq!(total, plain_total);
        assert_eq!(plain_mapped, plain_total);
        assert!(mapped < total);
        assert!(styled.capture(0).unwrap().iter().any(|&p| p < 160));
        assert_eq!(styled.restore_anchor(&old), Some(target));
    }

    #[test]
    fn visibility_keeps_search_selection_and_geometry_but_suppresses_own_paint() {
        let html = "<html><body><p>Before <span id='hidden' style='visibility:hidden'>GHOST <span style='visibility:visible'>CHILD</span></span> after.</p></body></html>";
        let mut plain = open(html, false);
        let mut styled = open(html, true);
        assert_eq!(styled.coverage(), plain.coverage());
        let anchor = styled.matches("GHOST").remove(0);
        assert_eq!(
            styled.source_presentation(&anchor),
            Some(SourcePresentation::VisibilityHidden)
        );
        assert_eq!(
            styled.fragment_presentation("hidden"),
            Some(SourcePresentation::VisibilityHidden)
        );
        assert_eq!(styled.fragment("hidden"), Some(anchor.clone()));
        assert_eq!(plain.matches("GHOST"), [anchor.clone()]);
        let end = crate::Anchor {
            offset: anchor.offset + "GHOST".len(),
            ..anchor.clone()
        };
        let selected = styled.select_range(&anchor, &end).unwrap();
        assert_eq!(selected.text, "GHOST");
        assert_eq!(
            styled.selection_rects(0, &selected),
            plain.selection_rects(0, &selected)
        );
        assert_eq!(
            styled.source_presentation(&styled.matches("CHILD")[0]),
            Some(SourcePresentation::Normal)
        );
        screenshot("visibility-visible", &mut plain);
        screenshot("visibility-hidden", &mut styled);
        let a = plain.capture(0).unwrap();
        let b = styled.capture(0).unwrap();
        assert!(a.iter().zip(&b).any(|(&x, &y)| x < y));
        assert!(!b.iter().zip(&a).any(|(&x, &y)| x < y));
    }

    #[test]
    fn pure_projected_images_keep_canonical_ordinals_and_omit_hidden_media() {
        let html = "<html><body><p><span style='display:none'>TEXT</span><img id='gone' style='display:none' src='../images/shore.png'/><img id='kept' src='../images/shore.png' width='100' height='100'/></p></body></html>";
        let probe = open(html, true);
        assert!(probe.fragment("gone").is_none());
        let anchor = probe.page_anchor(0).unwrap();
        assert_eq!(anchor.illustration, Some(1));
        assert!(probe.page_for(&anchor).is_some());
        let mut removed = anchor.clone();
        removed.illustration = Some(0);
        assert_eq!(
            probe.source_presentation(&removed),
            Some(SourcePresentation::CssOmitted)
        );
        assert!(probe.page_for(&removed).is_none());
        assert_eq!(probe.source.blocks[0].images.len(), 2);
    }

    #[test]
    fn fractional_table_fallback_is_explicit_readable_and_keeps_saved_source_ranges() {
        let html = "<html><body><table><colgroup><col width='30%'/><col width='30%'/><col width='40%'/></colgroup><tr><td rowspan='2' id='cut' style='display:none'>TRAP</td><td colspan='2' id='first'>FIRST retained</td></tr><tr><td id='second'>SECOND retained</td><td id='third'>THIRD retained</td></tr></table></body></html>";
        let plain = open(html, false);
        let mut styled = open(html, true);
        screenshot("hinted-table-source-order", &mut styled);
        assert_eq!(styled.source.revision, plain.source.revision);
        assert!(
            styled
                .layout_limitations()
                .iter()
                .any(|reason| reason.contains("column hints")),
            "{:?}",
            styled.layout_limitations()
        );
        assert!(styled.matches("TRAP").is_empty());
        assert!(styled.fragment("cut").is_none());
        let mut tops = Vec::new();
        for word in ["FIRST", "SECOND", "THIRD"] {
            let a = plain.matches(word)[0].clone();
            assert_eq!(styled.matches(word), [a.clone()]);
            let end = crate::Anchor {
                offset: a.offset + word.len(),
                ..a.clone()
            };
            assert_eq!(styled.select_range(&a, &end).unwrap().text, word);
            assert!(
                !styled
                    .selection_rects(
                        styled.page_for(&a).unwrap(),
                        &styled.select_range(&a, &end).unwrap()
                    )
                    .is_empty()
            );
            tops.push(styled.anchor_top(&a).unwrap());
        }
        assert!(tops.windows(2).all(|p| p[0] < p[1]), "{tops:?}");
    }
    #[test]
    fn ruby_display_removes_annotation_geometry_while_visibility_only_changes_paint() {
        let markup = |rule: &str| {
            format!(
                "<html><body><p>A <ruby>BOOK<rt style='{rule}'>READ</rt></ruby> after.</p></body></html>"
            )
        };
        let mut shown = open(&markup("visibility:visible"), true);
        let mut hidden = open(&markup("visibility:hidden"), true);
        let mut removed = open(&markup("display:none"), true);
        assert_eq!(shown.coverage(), hidden.coverage());
        assert_eq!(shown.coverage(), removed.coverage());
        assert_eq!(
            shown.matches("BOOK")[0].offset,
            removed.matches("BOOK")[0].offset
        );
        let book = shown.matches("BOOK")[0].clone();
        let end = crate::Anchor {
            offset: book.offset + 4,
            ..book.clone()
        };
        let selection = shown.select_range(&book, &end).unwrap();
        let hidden_start = hidden.matches("BOOK")[0].clone();
        let hidden_end = crate::Anchor {
            offset: hidden_start.offset + 4,
            ..hidden_start.clone()
        };
        let hidden_selection = hidden.select_range(&hidden_start, &hidden_end).unwrap();
        assert_eq!(
            shown.selection_rects(0, &selection),
            hidden.selection_rects(0, &hidden_selection)
        );
        screenshot("ruby-visible", &mut shown);
        screenshot("ruby-hidden", &mut hidden);
        screenshot("ruby-removed", &mut removed);
        let a = shown.capture(0).unwrap();
        let b = hidden.capture(0).unwrap();
        assert_ne!(a, b);
        assert!(a.iter().zip(&b).any(|(&x, &y)| x < y));
        assert!(!removed.capture(0).unwrap().iter().all(|&x| x == 255));
        let block = &removed.source.blocks[0];
        assert!(
            removed
                .reading_flow
                .layout_block(&removed.source, block)
                .unwrap()
                .ruby
                .is_empty()
        );
        assert_eq!(block.ruby.len(), 1);
    }

    #[test]
    fn vertical_projection_maps_hit_selection_and_paint_back_to_canonical_bytes() {
        let markup = |visibility| {
            format!(
                "<html><head><style>p{{writing-mode:vertical-rl}}</style></head><body><p>BEFORE <span style='display:none'>SECRET</span><span style='visibility:{visibility}'>GHOST</span> AFTER <ruby>BOOK<rt style='display:none'>READ</rt></ruby></p><p style='display:none'><img src='../images/shore.png'/></p></body></html>"
            )
        };
        let mut hidden = open(&markup("hidden"), true);
        let mut visible = open(&markup("visible"), true);
        assert!(hidden.is_vertical());
        assert!(hidden.matches("SECRET").is_empty());
        assert_eq!(hidden.coverage(), visible.coverage());
        let ghost = hidden.matches("GHOST")[0].clone();
        assert_eq!(
            hidden.source_presentation(&ghost),
            Some(SourcePresentation::VisibilityHidden)
        );
        let end = crate::Anchor {
            offset: ghost.offset + 5,
            ..ghost.clone()
        };
        let selected = hidden.select_range(&ghost, &end).unwrap();
        assert_eq!(selected.text, "GHOST");
        let page = hidden.page_for(&ghost).unwrap();
        let rects = hidden.selection_rects(page, &selected);
        assert!(!rects.is_empty());
        let visible_start = visible.matches("GHOST")[0].clone();
        let visible_end = crate::Anchor {
            offset: visible_start.offset + 5,
            ..visible_start.clone()
        };
        let visible_selected = visible.select_range(&visible_start, &visible_end).unwrap();
        assert_eq!(rects, visible.selection_rects(page, &visible_selected));
        let r = rects[0];
        let hit = hidden
            .word_at(page, r.x as f32 + r.width as f32 / 2., r.y as f32 + 2.)
            .unwrap();
        assert_eq!(hit.text, "GHOST");
        screenshot("vertical-visible", &mut visible);
        screenshot("vertical-hidden", &mut hidden);
        let a = visible.capture(page).unwrap();
        let b = hidden.capture(page).unwrap();
        assert!(a.iter().zip(&b).any(|(&x, &y)| x < y));
        assert!(!b.iter().zip(&a).any(|(&x, &y)| x < y));
    }

    #[test]
    fn semantic_note_extraction_preserves_inline_hiding_but_drops_linked_css() {
        let context = crate::footnotes::ReferenceContext {
            kind: crate::footnotes::Reference::Explicit,
            label: "1".into(),
            in_prose: true,
            source_block: 0,
        };
        for id_on_aside in [true, false] {
            let (aside_id, p_id) = if id_on_aside {
                (" id='note'", "")
            } else {
                ("", " id='note'")
            };
            let xml = format!(
                "<html xmlns:epub='http://www.idpf.org/2007/ops'><body><aside{aside_id} epub:type='footnote' style='display:none'><p{p_id}>INLINE NOTE</p></aside></body></html>"
            );
            let excerpt = crate::footnotes::extract(
                &xml,
                "note",
                &context,
                std::cmp::Ordering::Equal,
                false,
                false,
            )
            .unwrap();
            assert_eq!(excerpt.contains("style='display:none'"), id_on_aside);
            let mut preview = open(&excerpt, true);
            assert_eq!(preview.matches("INLINE NOTE").is_empty(), id_on_aside);
            assert_eq!(
                preview.capture(0).unwrap().iter().all(|&x| x == 255),
                id_on_aside
            );
            assert!(
                preview
                    .source
                    .blocks
                    .iter()
                    .any(|block| block.text.contains("INLINE NOTE"))
            );
        }
        let xml = "<html xmlns:epub='http://www.idpf.org/2007/ops'><head><link rel='stylesheet' href='hide.css'/><style>aside{display:none}</style></head><body><aside epub:type='footnote' id='note'><p>LINKED NOTE</p></aside></body></html>";
        let excerpt = crate::footnotes::extract(
            xml,
            "note",
            &context,
            std::cmp::Ordering::Equal,
            false,
            false,
        )
        .unwrap();
        assert!(!excerpt.contains("hide.css") && !excerpt.contains("<style>"));
        let preview = open(&excerpt, true);
        assert_eq!(preview.matches("LINKED NOTE").len(), 1);
    }
    #[test]
    fn projected_word_copy_and_bidi_graphemes_use_rendered_text_without_changing_stored_quote() {
        let html = "<html><body><p>ca<span style='display:none'>TRAP</span>fe&#x301; <span dir='rtl'>ש<span style='display:none'>TRAP</span>לום</span> after.</p></body></html>";
        let probe = open(html, true);
        let block = &probe.source.blocks[0];
        let start = probe.anchor(block.key, 0);
        let end = probe.anchor(block.key, block.text.len());
        let selected = probe.select_range(&start, &end).unwrap();
        assert_eq!(selected.text, "cafe\u{301} שלום after.");
        assert!(
            probe
                .select_source_range(&start, &end)
                .unwrap()
                .text
                .contains("TRAP")
        );
        let cafe = probe.matches("cafe\u{301}")[0].clone();
        let text_end = crate::Anchor {
            offset: block.text.find(' ').unwrap(),
            ..cafe.clone()
        };
        let selection = probe.select_range(&cafe, &text_end).unwrap();
        assert_eq!(selection.text, "cafe\u{301}");
        let rect = probe.selection_rects(0, &selection)[0];
        let word = probe
            .word_at(
                0,
                rect.x as f32 + 3.,
                rect.y as f32 + rect.height as f32 / 2.,
            )
            .unwrap();
        assert_eq!(word.text, "cafe\u{301}");
        let (before, after) = probe.word_context(&word, 3).unwrap();
        assert!(!before.contains("TRAP") && !after.contains("TRAP"));
    }

    #[test]
    fn epub_pagebreak_fragment_uses_next_visible_text_after_publisher_css_hiding() {
        let html = "<html xmlns:epub='http://www.idpf.org/2007/ops'><head><style>.suppressed{display:none}</style></head><body><p><span epub:type='pagebreak' id='page_x' title='x'/><span class='suppressed'>Hidden prefix</span><span id='visible'>The visible destination.</span></p></body></html>";
        let probe = open(html, true);
        let pagebreak = probe.fragment("page_x").expect("pagebreak destination");
        let visible = probe
            .matches("The visible destination.")
            .into_iter()
            .next()
            .expect("visible text destination");
        assert_eq!(pagebreak, visible);
    }

    #[test]
    fn trailing_epub_pagebreak_falls_back_to_previous_visible_text() {
        let html = "<html xmlns:epub='http://www.idpf.org/2007/ops'><head><style>.suppressed{display:none}</style></head><body><p>Visible before the marker.</p><p><span epub:type='pagebreak' id='page_end' title='end'/><span class='suppressed'>Hidden after the marker.</span></p></body></html>";
        let probe = open(html, true);
        let pagebreak = probe.fragment("page_end").expect("trailing pagebreak");
        let visible = probe
            .matches("Visible before the marker.")
            .into_iter()
            .next()
            .expect("preceding visible text");
        let last_character = crate::Anchor {
            offset: visible.offset + "Visible before the marker.".len() - 1,
            ..visible.clone()
        };
        assert_eq!(probe.page_for(&pagebreak), probe.page_for(&last_character));
    }

    #[test]
    fn ordinary_empty_fragment_does_not_escape_its_retained_subtree() {
        let html = "<html><head><style>.suppressed{display:none}</style></head><body><p><span id='ordinary'/><span class='suppressed'>Hidden prefix</span><span id='visible'>The visible destination.</span></p></body></html>";
        let probe = open(html, true);
        assert!(probe.fragment("ordinary").is_none());
    }
}
