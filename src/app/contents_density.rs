//! Contents settings, including presentation and outline options.
use super::list_density::Scope;
use super::{Item, Reader};

impl Reader {
    pub(super) fn contents_settings_items(&self) -> Vec<Item> {
        let mut items = Vec::from(self.list_density_items(Scope::Contents));
        items.extend(self.contents_metadata_settings_items());
        items
    }
}

#[cfg(test)]
mod tests;
