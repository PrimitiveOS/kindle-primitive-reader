// Compile the fixed Liang patterns and explicit exceptions into immutable tables.
// The Kindle does not parse TeX or allocate a dictionary when it opens a book.
use icu_normalizer::DecomposingNormalizer;
use icu_properties::{CodePointMapData, props::GeneralCategory};
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

fn stock_word_rules(tex: &str, apostrophes: bool, include_exceptions: bool) -> Vec<String> {
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
    if include_exceptions && tex.contains("\\hyphenation{") {
        add_exceptions(&mut rules, tex, false, "5");
    }
    rules
}

fn is_mark(ch: char) -> bool {
    matches!(
        CodePointMapData::<GeneralCategory>::new().get(ch),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

// Normalize only runs between pattern weights: boundaries with explicit
// weights must retain their position while canonically equivalent unweighted
// combining sequences receive the same trie representation as input words.
fn decomposed_rules(tex: &str, tag: &str) -> Vec<String> {
    let omit_unsupported = tag == "grc";
    let nfd = DecomposingNormalizer::new_nfd();
    let mut rules = Vec::new();
    for pattern in group(tex, "\\patterns{").split_whitespace() {
        let supported = pattern.chars().any(|ch| ch.is_ascii_digit())
            && pattern.chars().all(|ch| {
                ch.is_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '.'
                    || is_mark(ch)
                    || (tag == "ml"
                        && (ch.is_alphabetic() || matches!(ch, '\u{200c}' | '\u{200d}')))
                    || (omit_unsupported && matches!(ch, '\u{02bc}' | '\u{1fbd}' | '\u{1fbf}'))
            });
        if !supported && omit_unsupported {
            continue;
        }
        assert!(supported, "Unsupported decomposed hyphenation pattern");
        let mut normalized = String::new();
        let mut segment = String::new();
        for ch in pattern.chars() {
            if ch.is_ascii_digit() {
                normalized.push_str(&nfd.normalize(&segment));
                segment.clear();
                normalized.push(ch);
            } else {
                segment.push(ch);
            }
        }
        normalized.push_str(&nfd.normalize(&segment));
        rules.push(normalized);
    }
    rules
}

// Preserve KOReader's exact German 2023 data; XML comments contain disabled
// patterns and must not be included. The source is fixed and bounded.
fn pinned_german_rules() -> Vec<String> {
    let path = "assets/hyphenation/koreader-german-2023.pattern";
    println!("cargo:rerun-if-changed={path}");
    assert!(
        fs::metadata(path).expect("German source metadata").len() <= 1_200_000,
        "German pattern source limit exceeded"
    );
    let xml = fs::read_to_string(path).expect("Read pinned German patterns");
    assert!(
        xml.len() <= 1_200_000,
        "German pattern source limit exceeded"
    );
    let mut active = String::with_capacity(xml.len());
    let mut remaining = xml.as_str();
    while let Some((before, comment)) = remaining.split_once("<!--") {
        active.push_str(before);
        remaining = comment
            .split_once("-->")
            .expect("Unclosed German XML comment")
            .1;
    }
    assert!(
        !remaining.contains("-->"),
        "Unexpected German XML comment terminator"
    );
    active.push_str(remaining);
    let mut rest = active.as_str();
    let mut rules = Vec::new();
    while let Some((_, after_open)) = rest.split_once("<pattern>") {
        let (entry, after_close) = after_open
            .split_once("</pattern>")
            .expect("Unclosed German pattern");
        assert!(
            rules.len() < 40_000 && entry.len() <= 128,
            "German pattern limit exceeded"
        );
        let core = entry.trim_matches(' ');
        assert!(
            !core.is_empty()
                && core.chars().any(|ch| ch.is_ascii_digit())
                && core
                    .chars()
                    .all(|ch| ch.is_lowercase() || ch.is_ascii_digit())
                && !entry.starts_with("  ")
                && !entry.ends_with("  "),
            "Unsupported German pattern"
        );
        let mut pattern = String::new();
        if entry.starts_with(' ') {
            pattern.push('.');
        }
        pattern.push_str(core);
        if entry.ends_with(' ') {
            pattern.push('.');
        }
        rules.push(pattern);
        rest = after_close;
    }
    assert_eq!(
        rules.len(),
        36_729,
        "Unexpected pinned German pattern count"
    );
    rules
}

// The upstream Esperanto XML represents TeX word boundaries as edge spaces.
// Retain the original file and reject unfamiliar markup rather than silently
// compiling an incomplete or malformed dictionary.
fn esperanto_rules() -> Vec<String> {
    let path = "assets/hyphenation/eo.pattern";
    println!("cargo:rerun-if-changed={path}");
    let xml = fs::read_to_string(path).expect("Read Esperanto pattern source");
    assert!(
        xml.len() <= 256 * 1024,
        "Esperanto pattern source is too large"
    );
    let mut rest = xml.as_str();
    let mut rules = Vec::new();
    let mut entries = 0usize;
    while let Some((_, after_open)) = rest.split_once("<pattern>") {
        let (entry, after_close) = after_open
            .split_once("</pattern>")
            .expect("Unclosed Esperanto pattern");
        assert!(
            entries < 4096 && entry.len() <= 256,
            "Esperanto pattern limit exceeded"
        );
        entries += 1;
        let core = entry.trim_matches(' ');
        assert!(
            !core.is_empty()
                && core
                    .chars()
                    .all(|ch| ch.is_alphabetic() || ch.is_ascii_digit()),
            "Unsupported Esperanto pattern"
        );
        if core.chars().any(|ch| ch.is_ascii_digit()) {
            let mut rule = String::new();
            if entry.starts_with(' ') {
                rule.push('.');
            }
            rule.push_str(core);
            if entry.ends_with(' ') {
                rule.push('.');
            }
            rules.push(rule);
        }
        rest = after_close;
    }
    assert_eq!(entries, 2009, "Unexpected Esperanto pattern source");
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
        ("bg", "BULGARIAN"),
        ("gl", "GALICIAN"),
        ("ga", "IRISH"),
        ("uk", "UKRAINIAN"),
        ("pl", "POLISH"),
        ("ru", "RUSSIAN"),
        ("sv", "SWEDISH"),
        ("ca", "CATALAN"),
        ("tr", "TURKISH"),
        ("hr", "CROATIAN"),
        ("no", "NORWEGIAN"),
        ("lt", "LITHUANIAN"),
        ("is", "ICELANDIC"),
        ("oc", "OCCITAN"),
        ("cy", "WELSH"),
        ("pms", "PIEDMONTESE"),
        ("rm", "ROMANSH"),
        ("eu", "BASQUE"),
        ("fur", "FRIULIAN"),
        ("ka", "GEORGIAN"),
        ("grc", "GREEK"),
        ("ml", "MALAYALAM"),
        ("la-x-classic", "LATIN"),
        ("la-x-liturgic", "LITURGICAL_LATIN"),
        ("sk", "SLOVAK"),
        ("sl", "SLOVENIAN"),
    ] {
        let tex = tex_source(&format!("assets/hyphenation/hyph-{tag}.tex"));
        let mut rules = if matches!(tag, "la-x-classic" | "la-x-liturgic" | "grc" | "ml") {
            decomposed_rules(&tex, tag)
        } else {
            // Pinned Russian and Catalan omit TeX exceptions. Irish and
            // Norwegian retain only the exceptions selected below.
            stock_word_rules(&tex, false, !matches!(tag, "ru" | "ca" | "ga" | "no"))
        };
        if tag == "ga" {
            let exceptions = group(&tex, "\\hyphenation{");
            for word in ["dtiom-áintí", "thiom-áintí"] {
                assert_eq!(
                    exceptions
                        .split_whitespace()
                        .filter(|entry| *entry == word)
                        .count(),
                    1,
                    "Expected Irish exception is missing or duplicated"
                );
                rules.push(word.replace('-', "5"));
            }
        }
        if tag == "no" {
            // CRE's converter omits the final unspaced source pattern.
            assert_eq!(
                rules.iter().filter(|rule| rule.as_str() == "åøy4").count(),
                1,
                "Expected Norwegian source pattern missing or duplicated"
            );
            rules.retain(|rule| rule != "åøy4");
            // Pinned exceptions without breaks are omitted; CRE lowercases
            // each XML pattern, including uppercase exception entries.
            let exceptions = group(&tex, "\\hyphenation{");
            let mut included = 0usize;
            for word in exceptions.split_whitespace() {
                if word.contains('-') {
                    rules.push(word.to_lowercase().replace('-', "5"));
                    included += 1;
                }
            }
            assert_eq!(included, 90, "Unexpected Norwegian exception selection");
        }
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
    output.push_str(&compile(
        pinned_german_rules(),
        "GERMAN",
        &out_dir.join("de-1996_rules.txt"),
    ));
    output.push_str(&compile(
        esperanto_rules(),
        "ESPERANTO",
        &out_dir.join("eo_rules.txt"),
    ));
    let hungarian_path = "assets/hyphenation/hyph-hu.tex";
    assert!(
        fs::metadata(hungarian_path)
            .expect("Hungarian source metadata")
            .len()
            <= 1_048_576,
        "Hungarian source limit exceeded"
    );
    let hungarian = tex_source(hungarian_path);
    assert!(
        hungarian.len() <= 1_048_576,
        "Hungarian source limit exceeded"
    );
    let hungarian_rules = stock_word_rules(&hungarian, false, true);
    assert_eq!(
        hungarian_rules.len(),
        62_851,
        "Unexpected Hungarian pattern count"
    );
    output.push_str(&compile_hungarian(
        hungarian_rules,
        &out_dir.join("hu_rules.txt"),
    ));
    fs::write(out_dir.join("hyphenation_patterns.rs"), output).unwrap();
}

fn compile(mut rules: Vec<String>, name: &str, normalized: &Path) -> String {
    rules.sort_unstable();
    // A normalized, independently comparable build artifact, excluded from the package.
    // The modified Basque data carries its upstream notice even in local build
    // artifacts; parsers comparing that artifact should skip comment lines.
    let notice = if name == "BASQUE" {
        include_str!("docs/licenses/HYPHENATION-EU.txt")
            .lines()
            .map(|line| format!("# {line}\n"))
            .collect::<String>()
    } else {
        String::new()
    };
    let fingerprint_input = notice + &rules.join("\n") + "\n";
    fs::write(normalized, fingerprint_input).unwrap();

    let mut nodes = vec![Node::default()];
    for pattern in rules {
        let mut at = 0;
        let mut weights = vec![0];
        for ch in pattern.chars() {
            if ch.is_ascii_digit() {
                *weights.last_mut().unwrap() = ch as u8 - b'0';
            } else {
                assert!(
                    ch.is_lowercase()
                        || ch == '.'
                        || ch == '\''
                        || is_mark(ch)
                        || (name == "GREEK" && matches!(ch, '\u{02bc}' | '\u{1fbd}' | '\u{1fbf}'))
                        || (name == "MALAYALAM"
                            && (ch.is_alphabetic() || matches!(ch, '\u{200c}' | '\u{200d}')))
                );
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
    let generated_notice = if name == "BASQUE" {
        include_str!("docs/licenses/HYPHENATION-EU.txt")
            .lines()
            .map(|line| format!("// {line}\n"))
            .collect::<String>()
    } else {
        String::new()
    };
    format!(
        "{generated_notice}const {name}: Patterns = Patterns {{ nodes:&{records:?}, edges:&{edges:?}, weights:&{weights:?} }};\n"
    )
}

// Hungarian exceeds compact 16-bit node indexes. Bound all build-time
// allocations and use checked 32-bit node, edge, weight and count indexes.
fn compile_hungarian(mut rules: Vec<String>, normalized: &Path) -> String {
    const MAX_NODES: usize = 200_000;
    const MAX_WEIGHTS: usize = 1_000_000;
    const MAX_PATTERN_SCALARS: usize = 64;
    rules.sort_unstable();
    let notice = include_str!("docs/licenses/HYPHENATION-HU.txt");
    let header = notice
        .lines()
        .map(|line| format!("# {line}\n"))
        .collect::<String>();
    fs::write(normalized, header + &rules.join("\n") + "\n").unwrap();
    let mut nodes = vec![Node::default()];
    for pattern in rules {
        assert!(
            pattern.chars().count() <= MAX_PATTERN_SCALARS,
            "Hungarian pattern too long"
        );
        let mut at = 0usize;
        let mut weights = vec![0u8];
        for ch in pattern.chars() {
            if ch.is_ascii_digit() {
                *weights.last_mut().unwrap() = ch as u8 - b'0';
            } else {
                assert!(
                    ch.is_lowercase() || ch == '.',
                    "Unsupported Hungarian character"
                );
                let next = nodes.len();
                let index = *nodes[at].edges.entry(ch).or_insert(next);
                if index == next {
                    assert!(nodes.len() < MAX_NODES, "Hungarian node limit exceeded");
                    nodes.push(Node::default());
                }
                at = index;
                weights.push(0);
            }
        }
        if nodes[at].weights.is_empty() {
            nodes[at].weights = weights;
        } else {
            for (old, new) in nodes[at].weights.iter_mut().zip(weights) {
                *old = (*old).max(new);
            }
        }
    }
    let mut records = Vec::with_capacity(nodes.len());
    let mut edges = Vec::with_capacity(nodes.len().saturating_sub(1));
    let mut weights = Vec::new();
    let mut weight_offsets = BTreeMap::new();
    for node in &nodes {
        let offset = *weight_offsets
            .entry(node.weights.as_slice())
            .or_insert_with(|| {
                let offset = u32::try_from(weights.len()).expect("Hungarian weight offset");
                assert!(
                    weights.len() + node.weights.len() <= MAX_WEIGHTS,
                    "Hungarian weight limit exceeded"
                );
                weights.extend_from_slice(&node.weights);
                offset
            });
        records.push((
            u32::try_from(edges.len()).expect("Hungarian edge offset"),
            u32::try_from(node.edges.len()).expect("Hungarian edge count"),
            offset,
            u32::try_from(node.weights.len()).expect("Hungarian weight count"),
        ));
        for (&ch, &target) in &node.edges {
            edges.push((ch, u32::try_from(target).expect("Hungarian target node")));
        }
        assert!(edges.len() < MAX_NODES, "Hungarian edge limit exceeded");
    }
    let header = notice
        .lines()
        .map(|line| format!("// {line}\n"))
        .collect::<String>();
    format!(
        "{header}const HUNGARIAN: WidePatterns = WidePatterns {{ nodes:&{records:?}, edges:&{edges:?}, weights:&{weights:?} }};\n"
    )
}
