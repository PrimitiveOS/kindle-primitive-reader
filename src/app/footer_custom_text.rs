//! KOReader-compatible custom reading-footer text and bounded template expansion.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

const MAX_BYTES: u64 = 1024;
pub(crate) const MAX_CHARS: usize = 120;
pub(crate) const MAX_REPETITIONS: u8 = 99;
const MAX_EXPANDED_BYTES: usize = 4096;

pub(crate) struct ClockTokens {
    pub(crate) date: String,
    pub(crate) short_date: String,
    pub(crate) time: String,
    pub(crate) time_with_seconds: String,
}

pub(crate) fn local_clock() -> Option<ClockTokens> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let seconds = libc::time_t::try_from(seconds).ok()?;
    let mut value = std::mem::MaybeUninit::<libc::tm>::uninit();
    // `localtime_r` writes to the caller-owned `tm`, which is valid for this scope.
    let value = unsafe {
        if libc::localtime_r(&seconds, value.as_mut_ptr()).is_null() {
            return None;
        }
        value.assume_init()
    };
    Some(ClockTokens {
        date: format!(
            "{:04}-{:02}-{:02}",
            value.tm_year + 1900,
            value.tm_mon + 1,
            value.tm_mday
        ),
        short_date: format!("{:02}-{:02}", value.tm_mon + 1, value.tm_mday),
        time: format!("{:02}:{:02}", value.tm_hour, value.tm_min),
        time_with_seconds: format!(
            "{:02}-{:02}-{:02}",
            value.tm_hour, value.tm_min, value.tm_sec
        ),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Preference {
    pub(crate) text: String,
    pub(crate) repetitions: u8,
}

impl Default for Preference {
    fn default() -> Self {
        Self {
            text: "KOReader".into(),
            repetitions: 1,
        }
    }
}

impl Preference {
    pub(crate) fn set_text(&mut self, value: &str) -> io::Result<()> {
        let text = if value.is_empty() { " " } else { value };
        validate_text(text)?;
        self.text = text.to_owned();
        Ok(())
    }

    pub(crate) fn set_repetitions(&mut self, repetitions: u8) -> io::Result<()> {
        if !(1..=MAX_REPETITIONS).contains(&repetitions) {
            return Err(invalid("Choose between 1 and 99 repetitions"));
        }
        self.repetitions = repetitions;
        Ok(())
    }

    pub(crate) fn is_merge_separator(&self) -> bool {
        self.text.chars().all(|character| character == ' ')
    }

    pub(crate) fn expand(&self, value: impl Fn(char) -> Option<String>) -> String {
        let mut expanded = String::new();
        for _ in 0..self.repetitions {
            let mut characters = self.text.chars().peekable();
            while let Some(character) = characters.next() {
                if character == '%' && characters.peek().is_some_and(char::is_ascii_alphabetic) {
                    let token = characters.next().unwrap();
                    push_limited(&mut expanded, &value(token).unwrap_or_else(|| "N/A".into()));
                } else {
                    push_char_limited(&mut expanded, character);
                }
                if expanded.len() >= MAX_EXPANDED_BYTES {
                    return expanded;
                }
            }
        }
        expanded
    }

    pub(crate) fn contains_token(&self, token: char) -> bool {
        let mut characters = self.text.chars().peekable();
        while let Some(character) = characters.next() {
            if character == '%' && characters.peek() == Some(&token) {
                characters.next();
                return true;
            }
        }
        false
    }
}

pub(crate) fn load(path: &Path) -> Preference {
    with_storage(|| {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_BYTES {
            return Err(invalid("Invalid custom footer text preference"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("Custom footer text preference is too large"));
        }
        let mut preference: Preference =
            serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        preference.normalize();
        preference.validate()?;
        Ok(preference)
    })
    .unwrap_or_default()
}

pub(crate) fn save(path: &Path, preference: &Preference) -> io::Result<()> {
    preference.validate()?;
    let bytes = serde_json::to_vec(preference).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Custom footer text preference is too large"));
    }
    primitive_app::save(path, &bytes)
}

pub(crate) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("The custom footer text backup is too large"));
    }
    let mut preference: Preference = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    preference.normalize();
    preference.validate()?;
    serde_json::to_value(preference).map_err(io::Error::other)
}

impl Preference {
    fn normalize(&mut self) {
        if self.text.is_empty() {
            self.text.push(' ');
        }
    }

    pub(crate) fn validate(&self) -> io::Result<()> {
        validate_text(&self.text)?;
        if !(1..=MAX_REPETITIONS).contains(&self.repetitions) {
            return Err(invalid("Custom footer text repetition count is invalid"));
        }
        Ok(())
    }
}

fn validate_text(text: &str) -> io::Result<()> {
    if text.chars().count() > MAX_CHARS || text.chars().any(char::is_control) {
        return Err(invalid(
            "Custom footer text must be 120 characters or fewer",
        ));
    }
    Ok(())
}

fn push_limited(output: &mut String, value: &str) {
    let remaining = MAX_EXPANDED_BYTES.saturating_sub(output.len());
    if value.len() <= remaining {
        output.push_str(value);
        return;
    }
    let mut boundary = remaining.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    output.push_str(&value[..boundary]);
}

fn push_char_limited(output: &mut String, character: char) {
    if output.len() + character.len_utf8() <= MAX_EXPANDED_BYTES {
        output.push(character);
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_koreaders_custom_footer() {
        assert_eq!(Preference::default().text, "KOReader");
        assert_eq!(Preference::default().repetitions, 1);
    }

    #[test]
    fn empty_text_becomes_a_mergeable_space_and_repetitions_are_bounded() {
        let mut preference = Preference::default();
        preference.set_text("").unwrap();
        assert_eq!(preference.text, " ");
        assert!(preference.is_merge_separator());
        preference.set_repetitions(MAX_REPETITIONS).unwrap();
        assert!(preference.set_repetitions(0).is_err());
    }

    #[test]
    fn expands_koreader_tokens_repeats_and_preserves_escaped_percent() {
        let mut preference = Preference::default();
        preference.set_text("%T · %c/%t · %% · %Z").unwrap();
        preference.set_repetitions(2).unwrap();
        let expanded = preference.expand(|token| match token {
            'T' => Some("Field Notes".into()),
            'c' => Some("3".into()),
            't' => Some("24".into()),
            _ => None,
        });
        assert_eq!(
            expanded,
            "Field Notes · 3/24 · %% · N/AField Notes · 3/24 · %% · N/A"
        );
    }

    #[test]
    fn expansion_is_bounded_even_when_metadata_and_repetitions_are_large() {
        let mut preference = Preference::default();
        preference.set_text("%T").unwrap();
        preference.set_repetitions(MAX_REPETITIONS).unwrap();
        let expanded = preference.expand(|_| Some("界".repeat(MAX_CHARS)));
        assert!(expanded.len() <= MAX_EXPANDED_BYTES);
        assert!(expanded.is_char_boundary(expanded.len()));
    }

    #[test]
    fn preference_round_trips_and_rejects_invalid_files() {
        let preference = Preference {
            text: "%T — %A".into(),
            repetitions: 3,
        };
        let bytes = serde_json::to_vec(&preference).unwrap();
        assert_eq!(
            backup_value(&bytes).unwrap(),
            serde_json::to_value(&preference).unwrap()
        );
        let invalid: Preference = serde_json::from_str(
            &serde_json::json!({
                "text": "line\nfeed",
                "repetitions": 1
            })
            .to_string(),
        )
        .unwrap();
        assert!(invalid.validate().is_err());
    }
}
