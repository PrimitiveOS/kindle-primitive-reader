//! Contents settings, including presentation and outline options.
use super::list_density::Scope;
use super::{Item, Reader};

impl Reader {
    pub(super) fn contents_settings_items(&self) -> Vec<Item> {
        let mut items = Vec::from(self.list_density_items(Scope::Contents));
        items.extend(self.contents_metadata_settings_items());
        items.extend(self.alternative_contents_setting());
        // These are built-in settings labels and generated numbers only.
        let language = self.controls.interface_language;
        for item in &mut items {
            item.title = language.contents_settings_text(&item.title);
            if let Some(detail) = &mut item.detail {
                *detail = language.contents_settings_text(detail);
            }
        }
        items
    }
}

#[cfg(test)]
mod tests;
