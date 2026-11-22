//! Publisher properties retained independently of the reading layout and its revision.
use roxmltree::Node;

#[derive(Debug, Default)]
pub(crate) struct Metadata {
    pub(crate) language: Option<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) description: Option<String>,
    pub(crate) series: Vec<String>,
}

impl Metadata {
    pub(crate) fn opf(node: Node<'_, '_>) -> Self {
        const DC: &str = "http://purl.org/dc/elements/1.1/";
        let field = |name| {
            node.children()
                .find(|node| node.has_tag_name((DC, name)))
                .and_then(|node| node.text())
                .map(str::to_owned)
        };
        Self {
            language: field("language"),
            description: field("description"),
            keywords: node
                .children()
                .filter(|node| node.has_tag_name((DC, "subject")))
                .filter_map(|node| node.text())
                .map(str::to_owned)
                .collect(),
            series: node
                .children()
                .filter(|node| {
                    node.has_tag_name("meta") && node.attribute("name") == Some("calibre:series")
                })
                .filter_map(|node| node.attribute("content"))
                .map(str::to_owned)
                .collect(),
        }
    }

    pub(crate) fn html(root: Node<'_, '_>) -> Self {
        let head = root.children().find(|node| node.has_tag_name("head"));
        let field = |name: &str| {
            head.into_iter()
                .flat_map(|node| node.children())
                .find(|node| {
                    node.has_tag_name("meta")
                        && node
                            .attribute("name")
                            .is_some_and(|saved| saved.eq_ignore_ascii_case(name))
                })
                .and_then(|node| node.attribute("content"))
                .map(str::to_owned)
        };
        Self {
            language: root
                .attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
                .or_else(|| root.attribute("lang"))
                .map(str::to_owned),
            keywords: field("keywords").into_iter().collect(),
            description: field("description"),
            series: Vec::new(),
        }
    }

    pub(crate) fn fb2(info: Node<'_, '_>, input: &str) -> Self {
        const NS: &str = "http://www.gribuser.ru/xml/fictionbook/2.0";
        let field = |name| {
            info.children()
                .find(|node| node.has_tag_name((NS, name)))
                .and_then(|node| node.text())
                .map(str::to_owned)
        };
        Self {
            language: field("lang"),
            keywords: field("keywords").into_iter().collect(),
            description: info
                .children()
                .find(|node| node.has_tag_name((NS, "annotation")))
                .map(|node| input[node.range()].to_owned()),
            series: info
                .children()
                .filter(|node| node.has_tag_name((NS, "sequence")))
                .filter_map(|node| node.attribute("name"))
                .map(str::to_owned)
                .collect(),
        }
    }
}
