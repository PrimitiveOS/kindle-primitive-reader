//! Signed adjustment for a requested weight unavailable in a selected font face.
use parley::FontData;
use skrifa::{FontRef, MetadataProvider, Tag, raw::TableProvider};
use std::collections::HashMap;

pub(crate) fn strength(
    font: &FontData,
    requested: f32,
    size: f32,
    declared: &HashMap<u64, f32>,
) -> f32 {
    let Ok(face) = FontRef::from_index(font.data.data(), font.index) else {
        return 0.0;
    };
    let actual = face
        .axes()
        .iter()
        .find(|axis| axis.tag() == Tag::new(b"wght") && axis.min_value() <= axis.max_value())
        .map(|axis| requested.clamp(axis.min_value(), axis.max_value()))
        .unwrap_or_else(|| {
            declared.get(&font.data.id()).copied().unwrap_or_else(|| {
                face.os2()
                    .ok()
                    .map(|os2| f32::from(os2.us_weight_class()))
                    .filter(|weight| (1.0..=1_000.0).contains(weight))
                    .unwrap_or(400.0)
            })
        });
    // The reference uses a 26.6 pixel strength. Keep its signed rounding before
    // outlining, including fractional steps and reductions below a real face.
    (size * (requested - actual) * 64.0 / 6_400.0).round() / 64.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use parley::fontique::Blob;
    use std::sync::Arc;

    #[test]
    fn a_declared_face_class_takes_priority_over_font_metadata_for_signed_synthesis() {
        let bytes = std::fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
        let font = FontData::new(Blob::new(Arc::new(bytes)), 0);
        let mut declared = HashMap::new();
        assert_eq!(strength(&font, 700.0, 42.0, &declared), 1.96875);
        declared.insert(font.data.id(), 700.0);
        assert_eq!(strength(&font, 700.0, 42.0, &declared), 0.0);
        assert_eq!(strength(&font, 725.0, 42.0, &declared), 0.171875);
        assert_eq!(strength(&font, 675.0, 42.0, &declared), -0.171875);
    }
}
