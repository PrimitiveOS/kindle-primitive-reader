use roxmltree::Node;

use crate::Result;

use super::body::{MAX_DEPTH, retained};

// Preserve the established text grouping first. If the converted output
// exceeds its limit, a second plan also bounds raw source markup.
const TEXT_BUDGET: usize = 384 * 1024;
const MARKUP_BUDGET: usize = 704 * 1024;

#[derive(Clone, Copy)]
pub(super) enum Piece<'a, 'i> {
    Whole(Node<'a, 'i>),
    Open {
        node: Node<'a, 'i>,
        tag: &'static str,
        continued: bool,
    },
    Close(&'static str),
}

pub(super) fn wrappers(name: &str) -> Option<&'static str> {
    Some(match name {
        "section" => "section",
        "notebody" => "aside",
        "notes" | "annotation" | "poem" | "stanza" | "div" => "div",
        "epigraph" | "blockquote" => "blockquote",
        _ => return None,
    })
}

#[derive(Default)]
struct Cost {
    text: usize,
    markup: usize,
}

impl Cost {
    fn of(node: Node<'_, '_>) -> Self {
        Self {
            text: node
                .descendants()
                .filter(Node::is_text)
                .filter_map(|node| node.text())
                .map(str::len)
                .sum(),
            markup: node.range().len(),
        }
    }

    fn add(&self, other: &Self) -> Self {
        Self {
            text: self.text + other.text,
            markup: self.markup + other.markup,
        }
    }

    fn fits(&self, markup_limit: Option<usize>) -> bool {
        self.text <= TEXT_BUDGET && markup_limit.is_none_or(|limit| self.markup <= limit)
    }
}

struct Container<'a, 'i> {
    node: Node<'a, 'i>,
    tag: &'static str,
    active: bool,
    emitted: bool,
}

#[derive(Default)]
struct Plan<'a, 'i> {
    completed: Vec<Vec<Piece<'a, 'i>>>,
    current: Vec<Piece<'a, 'i>>,
    ancestors: Vec<Container<'a, 'i>>,
    used: Cost,
    markup_limit: Option<usize>,
}

pub(super) fn chapters<'a, 'i>(
    root: Node<'a, 'i>,
    bounded_markup: bool,
) -> Result<Vec<Vec<Piece<'a, 'i>>>> {
    let mut plan = Plan {
        markup_limit: bounded_markup.then_some(MARKUP_BUDGET),
        ..Plan::default()
    };
    for element in root.children().filter(Node::is_element) {
        plan.visit(element, 0)?;
    }
    if !plan.current.is_empty() {
        plan.completed.push(plan.current);
    }
    if plan.completed.is_empty() {
        return Err("The FB3 has no reading sections".into());
    }
    Ok(plan.completed)
}

impl<'a, 'i> Plan<'a, 'i> {
    fn visit(&mut self, node: Node<'a, 'i>, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err("The FB3 exceeds its section nesting limit".into());
        }
        let cost = Cost::of(node);
        if cost.fits(self.markup_limit) {
            if !self.used.add(&cost).fits(self.markup_limit) {
                self.finish_current();
            }
            self.open_ancestors();
            self.used = self.used.add(&cost);
            self.current.push(Piece::Whole(node));
            return Ok(());
        }
        if node.is_element() && !retained(node) {
            return Ok(());
        }
        let tag = wrappers(node.tag_name().name())
            .ok_or("An indivisible FB3 block exceeds Reader's chapter budget")?;
        self.ancestors.push(Container {
            node,
            tag,
            active: false,
            emitted: false,
        });
        // Supported split wrappers contain block children. Leading formatting
        // whitespace must not establish a destination at the tail of a full chapter.
        for child in node.children().filter(|child| {
            child.is_element()
                || child
                    .text()
                    .is_some_and(|text| child.is_text() && !text.trim().is_empty())
        }) {
            self.visit(child, depth + 1)?;
        }
        let container = self.ancestors.pop().expect("matching open container");
        if container.active {
            self.current.push(Piece::Close(tag));
        }
        Ok(())
    }

    fn open_ancestors(&mut self) {
        for ancestor in self
            .ancestors
            .iter_mut()
            .filter(|ancestor| !ancestor.active)
        {
            self.current.push(Piece::Open {
                node: ancestor.node,
                tag: ancestor.tag,
                continued: ancestor.emitted,
            });
            ancestor.active = true;
            ancestor.emitted = true;
        }
    }

    fn finish_current(&mut self) {
        for ancestor in self
            .ancestors
            .iter_mut()
            .rev()
            .filter(|ancestor| ancestor.active)
        {
            self.current.push(Piece::Close(ancestor.tag));
            ancestor.active = false;
        }
        self.completed.push(std::mem::take(&mut self.current));
        self.used = Cost::default();
    }
}
