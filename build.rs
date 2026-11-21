// Compile the fixed Liang patterns and explicit exceptions into immutable tables.
// The Kindle does not parse TeX or allocate a dictionary when it opens a book.
use std::{collections::BTreeMap, env, fs, path::PathBuf};

#[derive(Default)]
struct Node {
    edges: BTreeMap<u8, usize>,
    weights: Vec<u8>,
}

fn tex_source(path: &str) -> String {
    println!("cargo:rerun-if-changed={path}");
    fs::read_to_string(path)
        .expect("English hyphenation source")
        .lines()
        .map(|line| line.split('%').next().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn group<'a>(tex: &'a str, command: &str) -> &'a str {
    tex.split_once(command)
        .unwrap()
        .1
        .split_once('}')
        .unwrap()
        .0
}

fn main() {
    let tex = tex_source("assets/hyphenation/hyph-en-us.tex");
    let corrections = tex_source("assets/hyphenation/ushyphex.tex");
    let mut rules: Vec<String> = group(&tex, "\\patterns{")
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    for (source, whole_word, priority) in [(&tex, false, "5"), (&corrections, true, "9")] {
        for entry in group(source, "\\hyphenation{").split_whitespace() {
            let word = entry.to_ascii_lowercase();
            assert!(word.bytes().all(|ch| ch.is_ascii_lowercase() || ch == b'-'));
            if word.contains('-') {
                let weighted = word.replace('-', priority);
                rules.push(if whole_word {
                    format!(".{weighted}.")
                } else {
                    weighted
                });
            } else {
                let mut pattern = String::from(".");
                for (index, letter) in word.chars().enumerate() {
                    if index > 0 {
                        pattern.push('8');
                    }
                    pattern.push(letter);
                }
                pattern.push('.');
                rules.push(pattern);
            }
        }
    }
    // In the reference dictionary these short past-tense forms give the doubled
    // consonant an even weight. A stronger overlapping rule may still win.
    for word in ["added", "fitted", "mugged"] {
        let middle = word
            .as_bytes()
            .windows(2)
            .position(|letters| letters[0] == letters[1])
            .unwrap()
            + 1;
        rules.push(format!(".{}4{}.", &word[..middle], &word[middle..]));
    }

    rules.sort_unstable();
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    // A normalized, independently comparable build artifact, excluded from the package.
    let fingerprint_input = rules.join("\n") + "\n";
    fs::write(out_dir.join("english_rules.txt"), fingerprint_input).unwrap();

    let mut nodes = vec![Node::default()];
    for pattern in rules {
        let mut at = 0;
        let mut weights = vec![0];
        for ch in pattern.bytes() {
            if ch.is_ascii_digit() {
                *weights.last_mut().unwrap() = ch - b'0';
            } else {
                assert!(ch.is_ascii_lowercase() || ch == b'.');
                let next = nodes.len();
                let index = *nodes[at].edges.entry(ch).or_insert(next);
                if index == next {
                    nodes.push(Node::default());
                }
                at = index;
                weights.push(0);
            }
        }
        let existing = &mut nodes[at].weights;
        if existing.is_empty() {
            *existing = weights;
        } else {
            for (weight, incoming) in existing.iter_mut().zip(weights) {
                *weight = (*weight).max(incoming);
            }
        }
    }
    let mut records = Vec::new();
    let mut edges = Vec::new();
    let mut weights = Vec::new();
    for node in &nodes {
        records.push((
            u16::try_from(edges.len()).unwrap(),
            u8::try_from(node.edges.len()).unwrap(),
            u16::try_from(weights.len()).unwrap(),
            u8::try_from(node.weights.len()).unwrap(),
        ));
        edges.extend(
            node.edges
                .iter()
                .map(|(&key, &index)| (key, u16::try_from(index).unwrap())),
        );
        weights.extend_from_slice(&node.weights);
    }
    let output = format!(
        "const NODES: &[(u16,u8,u16,u8)] = &{records:?};\nconst EDGES: &[(u8,u16)] = &{edges:?};\nconst WEIGHTS: &[u8] = &{weights:?};\n"
    );
    fs::write(out_dir.join("english_patterns.rs"), output).unwrap();
}
