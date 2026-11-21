// Compile the fixed Liang patterns and explicit exceptions into immutable tables.
// The Kindle does not parse TeX or allocate a dictionary when it opens a book.
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

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

fn add_exceptions(rules: &mut Vec<String>, source: &str, whole_word: bool, priority: &str) {
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

fn base_rules(tex: &str) -> Vec<String> {
    let mut rules: Vec<_> = group(tex, "\\patterns{")
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    add_exceptions(&mut rules, tex, false, "5");
    rules
}

fn main() {
    let american = tex_source("assets/hyphenation/hyph-en-us.tex");
    let british = tex_source("assets/hyphenation/hyph-en-gb.tex");
    let corrections = tex_source("assets/hyphenation/ushyphex.tex");
    let mut rules = base_rules(&american);
    add_exceptions(&mut rules, &corrections, true, "9");
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

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut output = compile(rules, "AMERICAN", &out_dir.join("english_rules.txt"));
    output.push_str(&compile(
        base_rules(&british),
        "BRITISH",
        &out_dir.join("british_english_rules.txt"),
    ));
    fs::write(out_dir.join("english_patterns.rs"), output).unwrap();
}

fn compile(mut rules: Vec<String>, name: &str, normalized: &Path) -> String {
    rules.sort_unstable();
    // A normalized, independently comparable build artifact, excluded from the package.
    let fingerprint_input = rules.join("\n") + "\n";
    fs::write(normalized, fingerprint_input).unwrap();

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
    format!(
        "const {name}: Patterns = Patterns {{ nodes:&{records:?}, edges:&{edges:?}, weights:&{weights:?} }};\n"
    )
}
