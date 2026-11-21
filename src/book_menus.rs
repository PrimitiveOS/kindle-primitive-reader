//! Stable per-document identifiers for the last open reading panels.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TopGroup {
    Navigation,
    Appearance,
    Settings,
    Tools,
    Search,
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BarPanel {
    Pages,
    Text,
    TextLayout,
    TextStyle,
    TextScreen,
    TextSpacing,
    TextRendering,
    View,
    ViewLayout,
    ViewScreen,
    Scan,
}
// Older builds may encounter a panel added by a newer version.
fn known<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(name.and_then(|name| serde_json::from_value(serde_json::Value::String(name)).ok()))
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BookMenus {
    #[serde(deserialize_with = "known", skip_serializing_if = "Option::is_none")]
    pub top: Option<TopGroup>,
    #[serde(deserialize_with = "known", skip_serializing_if = "Option::is_none")]
    pub bottom: Option<BarPanel>,
}

impl BookMenus {
    pub fn is_empty(&self) -> bool {
        self.top.is_none() && self.bottom.is_none()
    }
}
