//! Native subsets of KOReader's selectable User-Agent stylesheets.
//!
//! These rules are authored for PrimitiveOS's supported cascade instead of
//! bundling KOReader's GPL stylesheet assets. Unsupported CSS properties remain
//! covered by the documented renderer limits.

use crate::BaseStylesheet;

pub(super) fn css(stylesheet: BaseStylesheet) -> &'static str {
    match stylesheet {
        BaseStylesheet::Auto | BaseStylesheet::None => "",
        BaseStylesheet::Traditional => TRADITIONAL,
        BaseStylesheet::HtmlStandard => HTML_STANDARD,
        BaseStylesheet::FictionBook => FICTIONBOOK,
    }
}

pub(super) fn traditional_tables(stylesheet: BaseStylesheet, enabled: bool) -> &'static str {
    if stylesheet != BaseStylesheet::Traditional || !enabled {
        return "";
    }
    "
    table { font-size:80%; margin-top:3px; margin-bottom:3px; border-spacing:1px; }
    table table { font-size:100%; }
    td, th { padding:3px; }
    th { background-color:#ddd; font-weight:bold; text-align:center; }
    table caption { padding:4px; background-color:#eee; }
    thead, tbody, tfoot, table > tr { vertical-align:middle; }
    "
}

// KOReader's epub.css extends HTML defaults with justified prose, indented
// paragraphs, book-like heading scales, and restrained block quotations.
const TRADITIONAL: &str = r#"
body { text-align: justify; }
h1, h2, h3, h4, h5, h6 { text-align: center; hyphens: none; }
h1 { font-size: 150%; }
h2 { font-size: 140%; }
h3 { font-size: 130%; }
h4 { font-size: 120%; }
h5 { font-size: 110%; }
h6 { font-size: 100%; }
p { text-indent: 1.2em; }
blockquote { padding: 0.5em 1em 0.5em 2em; }
ol, ul { padding-left: 1em; }
"#;

// Supported HTML Standard defaults most visible in a long-form text reader.
// Paragraph margins are represented as bottom padding because the native block
// model has no collapsing margins; one em between paragraphs avoids doubling.
const HTML_STANDARD: &str = r#"
h1 { font-size: 200%; }
h2 { font-size: 150%; }
h3 { font-size: 117%; }
h4 { font-size: 100%; }
h5 { font-size: 83%; }
h6 { font-size: 67%; }
p { padding-bottom: 1em; }
blockquote, figure { padding: 1em 40px; }
address { font-style: italic; }
pre, listing, plaintext, xmp { font-family: monospace; padding-top: 1em; padding-bottom: 1em; }
"#;

// FictionBook content is normalized to XHTML at import. The preserved class
// names retain the distinctions that KOReader's fb2.css uses for verse, poems,
// epigraphs, citations, annotations, dates, and text authors.
const FICTIONBOOK: &str = r#"
body { text-align: left; text-indent: 0; page-break-after: always; }
p { text-align: justify; text-indent: 1.2em; }
li { text-indent: 0; }
h1, h2, h3, h4, h5, h6 { text-align: center; text-indent: 0; font-weight: bold; hyphens: none; }
h1, h2 { page-break-before: always; page-break-inside: avoid; page-break-after: avoid; }
h3, h4, h5, h6 { page-break-inside: avoid; page-break-after: avoid; }
h3 { font-style: italic; }
blockquote.fb2-epigraph { text-align: left; font-style: italic; padding: 1em 1em 1em 15%; }
blockquote.fb2-cite { text-align: justify; font-style: italic; padding: 1em 5%; }
div.fb2-poem { text-indent: 0; padding-top: 1em; padding-bottom: 1em; }
div.fb2-stanza { text-align: left; font-style: italic; padding-left: 15%; padding-top: 1em; }
p.fb2-verse { text-align: left; text-indent: 1em hanging; hyphens: none; }
p.fb2-text-author { font-weight: bold; font-style: italic; padding-left: 5%; }
p.fb2-date { font-style: italic; padding-left: 5%; }
div.fb2-annotation { font-size: 80%; font-style: italic; text-align: justify; text-indent: 1em; padding-left: 5%; padding-right: 5%; }
"#;
