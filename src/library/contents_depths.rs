//! Per-book exclusions keyed by the provider's original Contents depth.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct ContentsDepths {
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    ignored: BTreeSet<usize>,
    #[serde(skip_serializing_if = "super::is_false")]
    pub(crate) bind_navigation: bool,
    #[serde(skip_serializing_if = "super::is_false")]
    pub(crate) bind_title: bool,
}

impl ContentsDepths {
    pub(crate) fn is_default(&self) -> bool {
        !self.bind_navigation && !self.bind_title && self.ignored.is_empty()
    }

    pub(crate) fn has_ignored(&self) -> bool {
        !self.ignored.is_empty()
    }

    pub(crate) fn includes(&self, depth: usize) -> bool {
        !self.ignored.contains(&depth)
    }

    pub(crate) fn use_for_navigation(&self, depth: usize) -> bool {
        !self.bind_navigation || self.includes(depth)
    }

    pub(crate) fn clear_ignored(&mut self) {
        self.ignored.clear();
    }

    pub(crate) fn toggle(&mut self, depth: usize) {
        if !self.ignored.remove(&depth) {
            self.ignored.insert(depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::library::{BookState, merge_states};

    #[test]
    fn chapter_level_preferences_load_legacy_state_and_restore_new_books() {
        let local: BookState =
            serde_json::from_str(r#"{"id":7,"path":"book.epub","title":"Book"}"#).unwrap();
        assert!(local.contents_depths.use_for_navigation(1));
        assert!(!local.contents_depths.bind_title);
        assert!(
            serde_json::to_value(&local)
                .unwrap()
                .get("contents_depths")
                .is_none()
        );

        let mut source = local.clone();
        source.contents_depths.toggle(1);
        source.contents_depths.bind_title = true;
        assert!(source.contents_depths.use_for_navigation(1));
        source.contents_depths.bind_navigation = true;
        let source: BookState =
            serde_json::from_value(serde_json::to_value(source).unwrap()).unwrap();
        assert!(!source.contents_depths.use_for_navigation(1));
        assert!(source.contents_depths.use_for_navigation(0));
        assert!(source.contents_depths.bind_title);

        let (restored, _) = merge_states(&[], std::slice::from_ref(&source));
        assert_eq!(restored[0].contents_depths, source.contents_depths);
        let (merged, _) = merge_states(&[local], &[source]);
        assert!(merged[0].contents_depths.is_default());
    }
}
