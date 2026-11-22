//! Immutable font-free stylesheet context for reflow search workers.
use crate::{book_style::BookStyle, epub::Book, flow_view::ChapterFlow, source::Source};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlowConfig {
    pub publisher_styles: bool,
    pub user_styles: crate::user_styles::Snapshot,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            publisher_styles: true,
            user_styles: Default::default(),
        }
    }
}

impl FlowConfig {
    pub(super) fn chapter(
        &self,
        book: &mut Book,
        href: &str,
        source: &Source,
    ) -> crate::Result<ChapterFlow> {
        let styles =
            BookStyle::load_for_flow(book, href, source, self.publisher_styles, &self.user_styles)?;
        ChapterFlow::new(
            source,
            styles
                .compute(&source.elements)
                .into_iter()
                .map(|style| style.flow)
                .collect(),
        )
    }
}
