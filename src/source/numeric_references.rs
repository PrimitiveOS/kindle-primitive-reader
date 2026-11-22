//! CREngine's numeric-reference compatibility, with pre-normalization caret history.
//! This does not convert literal C1 characters or text in CDATA sections.
use super::*;

const TEXT_MODEL: u64 = 0x943c_0f82_bab7_e351;

/// Defined Windows-1252 replacements used by CREngine's lvxml.cpp codeconvert.
fn converted(code: u32) -> Option<char> {
    Some(match code {
        128 => '€',
        130 => '‚',
        131 => 'ƒ',
        132 => '„',
        133 => '…',
        134 => '†',
        135 => '‡',
        136 => 'ˆ',
        137 => '‰',
        138 => 'Š',
        139 => '‹',
        140 => 'Œ',
        142 => 'Ž',
        145 => '‘',
        146 => '’',
        147 => '“',
        148 => '”',
        149 => '•',
        150 => '–',
        151 => '—',
        152 => '˜',
        153 => '™',
        154 => 'š',
        155 => '›',
        156 => 'œ',
        158 => 'ž',
        159 => 'Ÿ',
        _ => return None,
    })
}

pub(super) fn normalize(xml: &str) -> Option<String> {
    let mut output = String::new();
    let mut copied = 0;
    let mut at = 0;
    while at < xml.len() {
        let rest = &xml[at..];
        let skipped = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")]
            .into_iter()
            .find(|(start, _)| rest.starts_with(start));
        if let Some((start, end)) = skipped {
            at += rest[start.len()..]
                .find(end)
                .map_or(rest.len(), |n| start.len() + n + end.len());
            continue;
        }
        if rest.starts_with("&#") {
            let (prefix, radix) = if rest.starts_with("&#x") {
                (3, 16)
            } else {
                (2, 10)
            };
            let digits = rest[prefix..]
                .bytes()
                .take_while(|b| match radix {
                    16 => b.is_ascii_hexdigit(),
                    _ => b.is_ascii_digit(),
                })
                .count();
            if digits > 0
                && rest.as_bytes().get(prefix + digits) == Some(&b';')
                && let Ok(code) = u32::from_str_radix(&rest[prefix..prefix + digits], radix)
                && let Some(value) = converted(code)
            {
                use std::fmt::Write;
                output.push_str(&xml[copied..at]);
                write!(output, "&#{};", value as u32).unwrap();
                at += prefix + digits + 1;
                copied = at;
                continue;
            }
        }
        // Searching ASCII delimiters cannot land inside a UTF-8 scalar.
        at += 1;
        at += xml.as_bytes()[at..]
            .iter()
            .position(|b| matches!(b, b'<' | b'&'))
            .unwrap_or(xml.len() - at);
    }
    if copied == 0 {
        return None;
    }
    output.push_str(&xml[copied..]);
    Some(output)
}

#[derive(Clone)]
struct Projection {
    old_ruby: Option<LegacyOffsets>,
    old_len: usize,
    numeric: LegacyOffsets,
    new_flat_len: usize,
    new_ruby: Option<LegacyOffsets>,
}

pub(super) struct History {
    revision: u64,
    flat_revision: Option<u64>,
    blocks: BTreeMap<usize, Projection>,
}

impl History {
    pub(super) fn revisions(&self) -> impl Iterator<Item = u64> + '_ {
        std::iter::once(self.revision).chain(self.flat_revision)
    }
    pub(super) fn accepts(&self, revision: u64) -> bool {
        self.revisions().any(|r| r == revision)
    }
    pub(super) fn project(&self, block: &Block, revision: u64, offset: usize) -> Option<usize> {
        let projection = self.blocks.get(&block.key)?;
        let old_flat = if revision == self.revision {
            match &projection.old_ruby {
                Some(ruby) => ruby.unproject(offset, projection.old_len)?,
                None => offset,
            }
        } else {
            offset
        };
        let new_flat = projection
            .numeric
            .project(old_flat, projection.new_flat_len)?;
        match &projection.new_ruby {
            Some(ruby) => ruby.project(new_flat, block.text.len()),
            None => Some(new_flat),
        }
    }
    pub(super) fn remap(mut self, keys: &BTreeMap<usize, usize>) -> Self {
        self.blocks = self
            .blocks
            .into_iter()
            .filter_map(|(key, value)| Some((*keys.get(&key)?, value)))
            .collect();
        self
    }
}

fn scalar_offsets(old: &str, current: &str) -> Result<LegacyOffsets> {
    let mut offsets = LegacyOffsets {
        len: old.len(),
        ..Default::default()
    };
    let mut now = current.char_indices();
    for (at, c) in old.char_indices() {
        let (to, d) = now
            .next()
            .ok_or("Numeric-reference history lost a scalar")?;
        if c != d && converted(c as u32) != Some(d) {
            return Err("Numeric-reference history changed unrelated text".into());
        }
        offsets.record_span(at..at + c.len_utf8(), to..to + d.len_utf8());
    }
    if now.next().is_some() {
        return Err("Numeric-reference history added a scalar".into());
    }
    Ok(offsets)
}

pub(super) fn parse(
    original: &str,
    normalized: String,
    language: Option<Language>,
    kindle: &BTreeSet<usize>,
) -> Result<Source> {
    let tracked = Extraction {
        ruby: true,
        origins: true,
    };
    let old = Source::parse_document(
        original,
        crate::revision(original),
        None,
        language,
        kindle,
        tracked,
    )?;
    let mut current = Source::parse_document(
        &normalized,
        crate::revision(&normalized),
        None,
        language,
        kindle,
        tracked,
    )?;
    let flat = Extraction {
        ruby: false,
        origins: true,
    };
    let old_flat = old
        .legacy_revision
        .is_some()
        .then(|| {
            Source::parse_document(
                original,
                crate::revision(original),
                None,
                language,
                kindle,
                flat,
            )
        })
        .transpose()?;
    let new_flat = current
        .legacy_revision
        .is_some()
        .then(|| {
            Source::parse_document(
                &normalized,
                crate::revision(&normalized),
                None,
                language,
                kindle,
                flat,
            )
        })
        .transpose()?;
    let old_by_origin: BTreeMap<_, _> = old.blocks.iter().map(|b| (b.origin, b)).collect();
    let old_flat: BTreeMap<_, _> = old_flat
        .as_ref()
        .unwrap_or(&old)
        .blocks
        .iter()
        .map(|b| (b.origin, b))
        .collect();
    let new_flat: BTreeMap<_, _> = new_flat
        .as_ref()
        .unwrap_or(&current)
        .blocks
        .iter()
        .map(|b| (b.origin, b))
        .collect();
    let mut history = History {
        revision: old.revision,
        flat_revision: old.legacy_revision,
        blocks: BTreeMap::new(),
    };
    let mut keys = BTreeMap::new();
    for block in &current.blocks {
        let key = if let Some(previous) = old_by_origin.get(&block.origin) {
            let before = old_flat
                .get(&block.origin)
                .ok_or("A historical block lost its flat text")?;
            let after = new_flat
                .get(&block.origin)
                .ok_or("A normalized block lost its flat text")?;
            history.blocks.insert(
                previous.key,
                Projection {
                    old_ruby: previous.legacy.clone(),
                    old_len: previous.text.len(),
                    numeric: scalar_offsets(&before.text, &after.text)?,
                    new_flat_len: after.text.len(),
                    new_ruby: block.legacy.clone(),
                },
            );
            previous.key
        } else {
            *old.origins
                .get(block.origin)
                .ok_or("A normalized block lost its publisher origin")?
        };
        keys.insert(block.key, key);
    }
    for block in &mut current.blocks {
        block.key = keys[&block.key];
    }
    for (key, _) in current.fragments.values_mut() {
        *key = keys[key];
    }
    for (_, key) in &mut current.insertions {
        *key = keys[key];
    }
    if !current
        .blocks
        .windows(2)
        .all(|pair| pair[0].key < pair[1].key)
    {
        return Err("Numeric-reference history reordered publisher blocks".into());
    }
    // Previously stored TOC targets may contain the pre-normalization attribute value.
    for (id, (key, offset)) in &old.fragments {
        if let Some(block) = current.blocks.iter().find(|b| b.key == *key)
            && let Some(offset) = history.project(block, old.revision, *offset)
        {
            current
                .fragments
                .entry(id.clone())
                .or_insert((*key, offset));
        }
    }
    current.numeric_history.push(history);
    current.revision ^= TEXT_MODEL;
    current.legacy_revision = current.legacy_revision.map(|r| r ^ TEXT_MODEL);
    current.origins.clear();
    if current.normalized_xhtml.is_none() {
        current.normalized_xhtml = Some(normalized);
    }
    Ok(current)
}

#[cfg(test)]
mod tests;
