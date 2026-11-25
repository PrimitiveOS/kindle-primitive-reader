//! Optional user-supplied Armenian patterns. TeX is treated as inert data.
use super::PatternView;
use std::collections::BTreeMap;

pub(crate) const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_RULES: usize = 4096;
const MAX_NODES: usize = 8192;
const MAX_PATTERN_BYTES: usize = 128;
const MAX_WEIGHTS: usize = u16::MAX as usize;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct OwnedPatterns {
    nodes: Vec<(u16, u8, u16, u8)>,
    edges: Vec<(char, u16)>,
    weights: Vec<u8>,
}

#[derive(Default)]
struct Node {
    edges: BTreeMap<char, usize>,
    weights: Vec<u8>,
}

impl OwnedPatterns {
    pub(crate) fn view(&self) -> PatternView<'_> {
        PatternView {
            nodes: &self.nodes,
            edges: &self.edges,
            weights: &self.weights,
        }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }
        let input = std::str::from_utf8(bytes).ok()?;
        let cleaned = input
            .lines()
            .map(|line| line.split('%').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let (prefix, tail) = cleaned.split_once("\\patterns{")?;
        let (group, suffix) = tail.split_once('}')?;
        if !prefix.trim().is_empty() || !suffix.trim().is_empty() {
            return None;
        }
        let mut nodes = vec![Node::default()];
        let mut count = 0usize;
        for pattern in group.split_whitespace() {
            count += 1;
            if count > MAX_RULES || pattern.len() > MAX_PATTERN_BYTES {
                return None;
            }
            let mut at = 0usize;
            let mut weights = vec![0u8];
            let mut letters = String::new();
            let mut seen_digit = false;
            let mut previous_digit = false;
            for ch in pattern.chars() {
                if ch.is_ascii_digit() {
                    if previous_digit {
                        return None;
                    }
                    *weights.last_mut()? = ch as u8 - b'0';
                    seen_digit = true;
                    previous_digit = true;
                    continue;
                }
                previous_digit = false;
                if ch != '.' && !(('\u{0561}'..='\u{0587}').contains(&ch) && ch.is_lowercase()) {
                    return None;
                }
                letters.push(ch);
                let next = nodes.len();
                let index = *nodes.get_mut(at)?.edges.entry(ch).or_insert(next);
                if index == next {
                    if nodes.len() >= MAX_NODES {
                        return None;
                    }
                    nodes.push(Node::default());
                }
                at = index;
                weights.push(0);
            }
            let inner = letters.trim_matches('.');
            if !seen_digit
                || inner.is_empty()
                || inner.contains('.')
                || letters.len() - inner.len() > 2
                || letters.starts_with("..")
                || letters.ends_with("..")
            {
                return None;
            }
            let existing = &mut nodes.get_mut(at)?.weights;
            if existing.is_empty() {
                *existing = weights;
            } else {
                if existing.len() != weights.len() {
                    return None;
                }
                for (old, new) in existing.iter_mut().zip(weights) {
                    *old = (*old).max(new);
                }
            }
        }
        if count == 0 {
            return None;
        }
        let mut result = Self::default();
        let mut offsets = BTreeMap::new();
        for node in &nodes {
            let offset = if let Some(offset) = offsets.get(node.weights.as_slice()) {
                *offset
            } else {
                if result.weights.len().checked_add(node.weights.len())? > MAX_WEIGHTS {
                    return None;
                }
                let offset = u16::try_from(result.weights.len()).ok()?;
                result.weights.extend_from_slice(&node.weights);
                offsets.insert(node.weights.as_slice(), offset);
                offset
            };
            result.nodes.push((
                u16::try_from(result.edges.len()).ok()?,
                u8::try_from(node.edges.len()).ok()?,
                offset,
                u8::try_from(node.weights.len()).ok()?,
            ));
            for (&ch, &target) in &node.edges {
                result.edges.push((ch, u16::try_from(target).ok()?));
            }
        }
        Some(result)
    }
}
