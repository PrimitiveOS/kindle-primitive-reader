//! Literal custom properties produced by the pinned KOReader BookInfo editor.
use super::{Parser, SidecarError, Value, read_fields};

pub(crate) const MAX_BYTES: usize = 256 * 1024;
const MAX_VALUE_BYTES: usize = 16 * 1024;

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct CustomMetadata {
    pub(crate) title: Option<String>,
    pub(crate) authors: Option<String>,
    pub(crate) series: Option<String>,
    pub(crate) series_index: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) keywords: Option<String>,
    pub(crate) description: Option<String>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<CustomMetadata, SidecarError> {
    if bytes.len() > MAX_BYTES {
        return Err(SidecarError::TooLarge);
    }
    let mut custom = None;
    let mut seen_custom = false;
    read_fields(bytes, &mut |key, value| {
        if key.as_deref() == Some(b"custom_props") {
            if seen_custom {
                return Err(SidecarError::DuplicateField("custom_props"));
            }
            seen_custom = true;
            match value {
                Value::Nil => {}
                Value::Table(table) => custom = Some(table),
                _ => return Err(SidecarError::InvalidField("custom_props")),
            }
        }
        Ok(())
    })?;
    let mut result = CustomMetadata::default();
    let Some(table) = custom else {
        return Ok(result);
    };
    let fields = [
        ("title", &mut result.title),
        ("authors", &mut result.authors),
        ("series", &mut result.series),
        ("series_index", &mut result.series_index),
        ("language", &mut result.language),
        ("keywords", &mut result.keywords),
        ("description", &mut result.description),
    ];
    let mut fields = fields.map(|(key, slot)| (key, slot, false));
    Parser {
        bytes: table,
        offset: 0,
        fields: 0,
    }
    .table(
        1,
        Some(&mut |key, value| {
            let Some((name, slot, seen)) = fields
                .iter_mut()
                .find(|(name, ..)| key.as_deref() == Some(name.as_bytes()))
            else {
                return Ok(());
            };
            if *seen {
                return Err(SidecarError::DuplicateField(name));
            }
            *seen = true;
            let parsed = match value {
                Value::Nil => None,
                Value::Number(number) if *name == "series_index" => Some(number.to_string()),
                Value::String(bytes) if *name != "series_index" => {
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|_| SidecarError::InvalidField(name))?;
                    if bytes.len() > MAX_VALUE_BYTES
                        || text
                            .chars()
                            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                    {
                        return Err(SidecarError::InvalidField(name));
                    }
                    Some(text.to_owned())
                }
                _ => return Err(SidecarError::InvalidField(name)),
            };
            **slot = parsed;
            Ok(())
        }),
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_metadata_reads_stock_literals_and_ignores_cached_publication_data() {
        let parsed = parse(include_bytes!(
            "../../tests/fixtures/koreader-custom-metadata.lua"
        ))
        .unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Coastal Ledger"));
        assert_eq!(parsed.authors.as_deref(), Some("Ada Scout\nJun Tide"));
        assert_eq!(parsed.series.as_deref(), Some("North Shelf"));
        assert_eq!(parsed.series_index.as_deref(), Some("2.5"));
        assert_eq!(parsed.keywords.as_deref(), Some("Tide gauge\nWaves"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("Notebook from \"the coast\"")
        );
        assert_eq!(parsed.language, None);
        let explicit_language = parse(b"return {custom_props={language='vi', title=nil}}").unwrap();
        assert_eq!(explicit_language.language.as_deref(), Some("vi"));
        assert_eq!(explicit_language.title, None);
        assert_eq!(
            parse(b"return {doc_props={title='Cached title'}}").unwrap(),
            CustomMetadata::default()
        );
    }

    #[test]
    fn custom_metadata_rejects_nonliteral_or_invalid_fields_as_one_record() {
        for unsafe_table in [
            b"return {custom_props={title='Recorded', title='Other'}}".as_slice(),
            b"return {custom_props=nil, custom_props={title='Recorded'}}",
            b"return {custom_props={title=true}}",
            b"return {custom_props={title='Recorded', series_index='2.5'}}",
            b"return {custom_props={title='Recorded', series_index=1e999}}",
            b"return {custom_props={title='Recorded', language='vi\\000'}}",
            b"return {custom_props={title='Recorded', authors='\\255'}}",
            b"return {custom_props={title='Recorded', auxiliary=execute()}}",
            b"return {custom_props={title='Recorded'}, doc_props={description='a'..'b'}}",
            b"return {custom_props={title='Recorded'}}; return {}",
        ] {
            assert!(parse(unsafe_table).is_err(), "{unsafe_table:?}");
        }
        assert_eq!(
            parse(&vec![b'x'; MAX_BYTES + 1]),
            Err(SidecarError::TooLarge)
        );
        let overlong = format!(
            "return {{custom_props={{description='{}'}}}}",
            "x".repeat(MAX_VALUE_BYTES + 1)
        );
        assert_eq!(
            parse(overlong.as_bytes()),
            Err(SidecarError::InvalidField("description"))
        );
    }
}
