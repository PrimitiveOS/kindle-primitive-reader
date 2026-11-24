//! Publication font families observed in the selected native layouts.
use crate::Renderer;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct Families(BTreeMap<String, Family>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Family {
    pub(crate) name: Arc<str>,
    pub(crate) used: bool,
}

impl Families {
    pub(crate) fn from_layout(registered: &HashMap<u64, Arc<str>>, renderer: &Renderer) -> Self {
        if registered.is_empty() {
            return Self::default();
        }
        let mut used = HashSet::new();
        let visit = |id| {
            if registered.contains_key(&id) {
                used.insert(id);
            }
        };
        match renderer {
            Renderer::Blitz(_) => {}
            Renderer::Parley(chapter) => chapter.for_each_shaped_font(visit),
            Renderer::Vertical(chapter) => chapter.for_each_shaped_font(visit),
        }
        let mut families = Self::default();
        for (id, name) in registered {
            families.insert(name.clone(), used.contains(id));
        }
        families
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Family> {
        self.0.values()
    }

    pub(crate) fn include_catalog(&mut self, names: Vec<Arc<str>>) {
        for name in names {
            self.insert(name, false);
        }
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        for family in other.iter() {
            self.insert(family.name.clone(), family.used);
        }
    }

    fn insert(&mut self, name: Arc<str>, used: bool) {
        self.0
            .entry(name.to_ascii_lowercase())
            .and_modify(|prior| {
                if name < prior.name {
                    prior.name = name.clone();
                }
                prior.used |= used;
            })
            .or_insert(Family { name, used });
    }
}
