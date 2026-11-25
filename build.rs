// Compile the fixed Liang patterns and explicit exceptions into immutable tables.
// The Kindle does not parse TeX or allocate a dictionary when it opens a book.
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Node {
    edges: BTreeMap<char, usize>,
    weights: Vec<u8>,
}

fn tex_source(path: &str) -> String {
    println!("cargo:rerun-if-changed={path}");
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("Read hyphenation source {path}: {error}"))
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
        let word = entry.to_lowercase();
        assert!(word.chars().all(|ch| ch.is_alphabetic() || ch == '-'));
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

fn stock_word_rules(tex: &str, apostrophes: bool) -> Vec<String> {
    let mut rules = group(tex, "\\patterns{")
        .split_whitespace()
        .filter(|entry| {
            entry.chars().any(|ch| ch.is_ascii_digit())
                && entry.chars().all(|ch| {
                    ch.is_alphabetic()
                        || ch.is_ascii_digit()
                        || ch == '.'
                        || (apostrophes && ch == '\'')
                })
        })
        .map(str::to_owned)
        .collect();
    if tex.contains("\\hyphenation{") {
        add_exceptions(&mut rules, tex, false, "5");
    }
    rules
}

fn build_pattern_engine() {
    println!("cargo:rerun-if-changed=src/search/pattern.cpp");
    println!("cargo:rerun-if-changed=vendor/srell/srell.hpp");
    let mut compiler = cc::Build::new();
    compiler
        .cpp(true)
        .std("c++14")
        .warnings(false)
        .include("vendor/srell")
        .file("src/search/pattern.cpp");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("musl") {
        compiler.cpp_link_stdlib("c++");
    } else if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // The Command Line Tools install libc++ headers inside the selected SDK.
        let sdk = env::var_os("SDKROOT").map(PathBuf::from).or_else(|| {
            let output = std::process::Command::new("xcrun")
                .arg("--show-sdk-path")
                .output()
                .ok()?;
            output
                .status
                .success()
                .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        });
        if let Some(headers) = sdk
            .map(|path| path.join("usr/include/c++/v1"))
            .filter(|path| path.is_dir())
        {
            compiler
                .flag("-isystem")
                .flag(headers.to_str().expect("SDK header path"));
        }
    }
    compiler.compile("reader_search_pattern");
}

fn main() {
    build_pattern_engine();
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
    for (tag, name) in [
        ("es", "SPANISH"),
        ("it", "ITALIAN"),
        ("pt", "PORTUGUESE"),
        ("et", "ESTONIAN"),
        ("fi", "FINNISH"),
        ("da", "DANISH"),
        ("nl", "DUTCH"),
        ("fr", "FRENCH"),
        ("de-1996", "GERMAN"),
        ("bg", "BULGARIAN"),
        ("gl", "GALICIAN"),
        ("ga", "IRISH"),
        ("uk", "UKRAINIAN"),
        ("pl", "POLISH"),
        ("ru", "RUSSIAN"),
        ("sv", "SWEDISH"),
        ("sk", "SLOVAK"),
        ("sl", "SLOVENIAN"),
    ] {
        let tex = tex_source(&format!("assets/hyphenation/hyph-{tag}.tex"));
        let mut rules = stock_word_rules(&tex, tag == "uk");
        if tag == "fi" {
            // CRE converted this two-entry TeX line into one pattern with an
            // internal word-edge marker, unreachable when matching one word.
            rules.retain(|rule| !matches!(rule.as_str(), "r2as" | "l2as"));
        }
        if tag == "sk" {
            // The pinned CREngine Slovak table explicitly comments out this
            // unhyphenated TeX exception; retain its stock behavior.
            rules.retain(|rule| rule != ".d8o8s8ť.");
        }
        output.push_str(&compile(
            rules,
            name,
            &out_dir.join(format!("{tag}_rules.txt")),
        ));
    }
    fs::write(out_dir.join("hyphenation_patterns.rs"), output).unwrap();
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
        for ch in pattern.chars() {
            if ch.is_ascii_digit() {
                *weights.last_mut().unwrap() = ch as u8 - b'0';
            } else {
                assert!(ch.is_lowercase() || ch == '.' || ch == '\'');
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
    // Identical weight sequences share one offset without changing the
    // contiguous slices used by the matcher. This keeps the Dutch table
    // within its compact 16-bit weight indexes.
    let mut weight_offsets = BTreeMap::new();
    for node in &nodes {
        let offset = *weight_offsets
            .entry(node.weights.as_slice())
            .or_insert_with(|| {
                let offset = u16::try_from(weights.len()).expect("hyphenation weight index");
                weights.extend_from_slice(&node.weights);
                offset
            });
        records.push((
            u16::try_from(edges.len()).unwrap(),
            u8::try_from(node.edges.len()).unwrap(),
            offset,
            u8::try_from(node.weights.len()).unwrap(),
        ));
        edges.extend(
            node.edges
                .iter()
                .map(|(&key, &index)| (key, u16::try_from(index).unwrap())),
        );
    }
    format!(
        "const {name}: Patterns = Patterns {{ nodes:&{records:?}, edges:&{edges:?}, weights:&{weights:?} }};\n"
    )
}
