//! Offline annotation exports with literal quotations and user notes.

pub(crate) mod kindle_clippings;

#[derive(Clone, Copy)]
pub(crate) enum Format {
    Text,
    Markdown,
    Html,
}

impl Format {
    pub(crate) const ALL: [Self; 3] = [Self::Text, Self::Markdown, Self::Html];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Text => "Plain text",
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
        }
    }

    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Markdown => "md",
            Self::Html => "html",
        }
    }

    pub(crate) fn render<'a>(
        self,
        title: &str,
        marks: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) -> String {
        self.render_with_bookmarks(title, marks, std::iter::empty())
    }

    /// Export saved page bookmarks separately from highlighted quotations.
    /// Labels describe locations and must not be presented as book excerpts.
    pub(crate) fn render_with_bookmarks<'a>(
        self,
        title: &str,
        marks: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
        bookmarks: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) -> String {
        let mut output = String::new();
        match self {
            Self::Text => {
                output.push_str(title);
                output.push_str("\n\n");
                for (quote, note) in marks {
                    output.push_str(quote);
                    output.push('\n');
                    if let Some(note) = note {
                        output.push_str(note);
                        output.push('\n');
                    }
                    output.push('\n');
                }
                for (label, note) in bookmarks {
                    output.push_str("Bookmark: ");
                    output.push_str(label);
                    output.push('\n');
                    if let Some(note) = note {
                        output.push_str("Note: ");
                        output.push_str(note);
                        output.push('\n');
                    }
                    output.push('\n');
                }
            }
            Self::Markdown => {
                output.push_str("# ");
                markdown_text(&mut output, title);
                output.push_str("\n\n");
                for (quote, note) in marks {
                    for line in quote.lines() {
                        output.push_str("> ");
                        markdown_text(&mut output, line);
                        output.push('\n');
                    }
                    output.push('\n');
                    if let Some(note) = note {
                        output.push_str("**Note**\n\n");
                        markdown_text(&mut output, note);
                        output.push_str("\n\n");
                    }
                }
                for (label, note) in bookmarks {
                    output.push_str("**Bookmark:** ");
                    markdown_text(&mut output, label);
                    output.push_str("\n\n");
                    if let Some(note) = note {
                        output.push_str("**Note**\n\n");
                        markdown_text(&mut output, note);
                        output.push_str("\n\n");
                    }
                }
            }
            Self::Html => {
                output
                    .push_str("<!doctype html>\n<html lang=\"en\"><meta charset=\"utf-8\"><title>");
                html_text(&mut output, title);
                output.push_str("</title><main><h1>");
                html_text(&mut output, title);
                output.push_str("</h1>\n");
                for (quote, note) in marks {
                    output.push_str("<article><blockquote><p>");
                    html_text(&mut output, quote);
                    output.push_str("</p></blockquote>");
                    if let Some(note) = note {
                        output.push_str("<p><strong>Note</strong><br>");
                        html_text(&mut output, note);
                        output.push_str("</p>");
                    }
                    output.push_str("</article>\n");
                }
                for (label, note) in bookmarks {
                    output.push_str("<article><p><strong>Bookmark</strong><br>");
                    html_text(&mut output, label);
                    output.push_str("</p>");
                    if let Some(note) = note {
                        output.push_str("<p><strong>Note</strong><br>");
                        html_text(&mut output, note);
                        output.push_str("</p>");
                    }
                    output.push_str("</article>\n");
                }
                output.push_str("</main></html>\n");
            }
        }
        output
    }
}

fn markdown_text(output: &mut String, value: &str) {
    for c in value.chars() {
        if c.is_ascii_punctuation() {
            output.push('\\');
        }
        output.push(c);
    }
}

pub(crate) fn html_text(output: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            '\n' => output.push_str("<br>\n"),
            _ => output.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_literal_markup_and_note_text_in_exports() {
        let marks = [("[quote](x)\n<em>more</em>", Some("a & b"))];
        let html = Format::Html.render("Book <title>", marks);
        assert!(html.contains("Book &lt;title&gt;"));
        assert!(html.contains("&lt;em&gt;more&lt;/em&gt;"));
        assert!(html.contains("a &amp; b"));
        let markdown = Format::Markdown.render("Book <title>", marks);
        assert!(markdown.contains("> \\[quote\\]\\(x\\)\n> \\<em\\>more"));
        let text = Format::Text.render("Book", marks);
        assert_eq!(text, "Book\n\n[quote](x)\n<em>more</em>\na & b\n\n");
    }
}
