//! Decode legacy TCR text into the existing bounded plain-text input.

use crate::Result;

const SIGNATURE: &[u8] = b"!!8-Bit!!";
const CODE_COUNT: usize = 256;
const MAX_CODE_BYTES: usize = u8::MAX as usize;
const MAX_TEXT_BYTES: usize = crate::loose::MAX_INPUT;
pub(crate) const MAX_INPUT: usize =
    SIGNATURE.len() + CODE_COUNT * (1 + MAX_CODE_BYTES) + MAX_TEXT_BYTES;

pub(crate) fn decode(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() > MAX_INPUT {
        return Err("The TCR file exceeds Reader's input limit".into());
    }
    let mut packed = input
        .strip_prefix(SIGNATURE)
        .ok_or("The TCR header is invalid")?;
    let mut dictionary: [&[u8]; CODE_COUNT] = [&[]; CODE_COUNT];
    for code in &mut dictionary {
        let (&length, bytes) = packed
            .split_first()
            .ok_or("The TCR dictionary is incomplete")?;
        (*code, packed) = bytes
            .split_at_checked(usize::from(length))
            .ok_or("A TCR dictionary entry is incomplete")?;
    }

    // Measure expansion before allocating. Unused dictionary slots may be empty.
    let output_len = packed
        .iter()
        .try_fold(0, |total, &code| {
            let total = total + dictionary[usize::from(code)].len();
            (total <= MAX_TEXT_BYTES).then_some(total)
        })
        .ok_or("The TCR text exceeds Reader's layout input limit")?;
    if output_len == 0 {
        return Err("The TCR document has no text".into());
    }
    let mut output = Vec::with_capacity(output_len);
    for &code in packed {
        output.extend_from_slice(dictionary[usize::from(code)]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(entries: &[&[u8]], payload: &[u8]) -> Vec<u8> {
        let mut bytes = SIGNATURE.to_vec();
        for entry in entries {
            bytes.push(u8::try_from(entry.len()).unwrap());
            bytes.extend_from_slice(entry);
        }
        bytes.resize(bytes.len() + CODE_COUNT - entries.len(), 0);
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn opens_frozen_calibre_fixture_and_reads_two_legacy_encoded_paragraphs() {
        // Calibre 9.5.0 compressed the original text as Windows-1252.
        // SHA-256 a918383808daa49e5cdd101e4f121b6175fcdd2df6ab406b3c33559cb9b67af3
        let fixture = crate::test_fixture("field-notes.tcr");
        assert_eq!(
            decode(&std::fs::read(&fixture).unwrap()).unwrap(),
            b"Field notes \x97 sunrise.\r\n\r\nThe caf\xe9 opens at sunrise. & <tide>\r\n"
        );
        assert_eq!(
            crate::library::DocumentFormat::for_path(std::path::Path::new("Field notes.TCR")),
            Some(crate::library::DocumentFormat::Tcr)
        );
        let mut book = crate::epub::Book::open(&fixture).unwrap();
        assert_eq!(book.title, "field-notes");
        assert_eq!(book.chapters.len(), 1);
        let xhtml = book.chapter(0).unwrap();
        let chapter = crate::source::Source::parse(&xhtml).unwrap();
        assert_eq!(chapter.blocks.len(), 2);
        assert_eq!(chapter.blocks[0].text, "Field notes — sunrise.");
        assert_eq!(
            chapter.blocks[1].text,
            "The café opens at sunrise. & <tide>"
        );
    }

    #[test]
    fn rejects_invalid_signature_truncated_dictionary_or_empty_text() {
        let mut truncated_entry = SIGNATURE.to_vec();
        truncated_entry.extend_from_slice(&[3, b'a']);
        assert!(decode(&truncated_entry).is_err());
        assert!(decode(SIGNATURE).is_err());
        assert!(decode(b"Plain text").is_err());
        assert!(decode(&codes(&[], &[])).is_err());
        assert!(decode(&codes(&[b""], &[0])).is_err());
        assert_eq!(decode(&codes(&[b"", b"a"], &[0, 1, 0])).unwrap(), b"a");
    }

    #[test]
    fn caps_encoded_size_and_rejects_expansion_before_allocating_text() {
        let chunk = [b'a'; MAX_CODE_BYTES];
        let tail = vec![b'b'; MAX_TEXT_BYTES % MAX_CODE_BYTES];
        let mut indices = vec![0; MAX_TEXT_BYTES / MAX_CODE_BYTES];
        indices.push(1);
        let mut input = codes(&[&chunk, &tail], &indices);
        assert_eq!(decode(&input).unwrap().len(), MAX_TEXT_BYTES);
        input.push(0);
        assert!(decode(&input).is_err());
        assert!(decode(&vec![0; MAX_INPUT + 1]).is_err());
    }
}
