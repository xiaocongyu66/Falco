//! DOM — Document Object Model.
//!
//! A minimal but real DOM: Element nodes (with tag + attributes), Text nodes,
//! Comment nodes, and a Document root. Siblings are stored as a Vec; children
//! live inside Element. This avoids the classic `Rc<RefCell<Vec<Node>>>` cycle
//! headache and lets the layout engine traverse by reference efficiently.
//!
//! # Spec-compliant replacement
//!
//! The [`spec`] submodule contains the WHATWG-DOM-compliant replacement
//! (live parent/child/sibling links via `Rc<RefCell<Node>>`, MutationObserver,
//! Shadow DOM, custom elements, accessibility tree). It is **not yet wired
//! into the render pipeline** — `render_with_base_url` still uses the legacy
//! types in this file. See [`spec`] for details.

pub mod spec;

use std::collections::HashMap;

/// A DOM node. We use a single enum rather than a trait object — matches how
/// browsers actually store DOM internally (discriminated unions are fast).
#[derive(Debug, Clone)]
pub enum Node {
    Document(DocumentData),
    Element(ElementData),
    Text(TextData),
    Comment(String),
    Doctype(DoctypeData),
}

#[derive(Debug, Clone)]
pub struct DocumentData {
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct ElementData {
    pub tag: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct TextData {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DoctypeData {
    pub name: String,
}

impl Node {
    pub fn element(tag: impl Into<String>) -> Self {
        Node::Element(ElementData {
            tag: tag.into().to_lowercase(),
            attrs: HashMap::new(),
            children: Vec::new(),
        })
    }

    pub fn text(s: impl Into<String>) -> Self {
        Node::Text(TextData { text: s.into() })
    }

    pub fn as_element(&self) -> Option<&ElementData> {
        if let Node::Element(e) = self {
            Some(e)
        } else {
            None
        }
    }

    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        if let Node::Element(e) = self {
            Some(e)
        } else {
            None
        }
    }

    pub fn as_text(&self) -> Option<&TextData> {
        if let Node::Text(t) = self {
            Some(t)
        } else {
            None
        }
    }

    /// True if this node is "inline" by default (per HTML spec heuristics).
    /// Used by layout to decide block vs inline flow at the root level.
    pub fn is_inline(&self) -> bool {
        match self {
            Node::Text(_) => true,
            Node::Element(e) => matches!(
                e.tag.as_str(),
                "a" | "abbr"
                    | "b"
                    | "bdi"
                    | "bdo"
                    | "br"
                    | "cite"
                    | "code"
                    | "dfn"
                    | "em"
                    | "i"
                    | "kbd"
                    | "label"
                    | "mark"
                    | "q"
                    | "rp"
                    | "rt"
                    | "ruby"
                    | "s"
                    | "samp"
                    | "small"
                    | "span"
                    | "strong"
                    | "sub"
                    | "sup"
                    | "time"
                    | "u"
                    | "var"
                    | "wbr"
                    | "img"
                    | "input"
            ),
            _ => false,
        }
    }

    /// Tag is a "void" element — has no closing tag and no children.
    pub fn is_void_tag(tag: &str) -> bool {
        matches!(
            tag,
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
    }
}

impl ElementData {
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(|s| s.as_str())
    }

    pub fn has_class(&self, class: &str) -> bool {
        if let Some(c) = self.attrs.get("class") {
            c.split_whitespace().any(|c| c == class)
        } else {
            false
        }
    }

    pub fn id(&self) -> Option<&str> {
        self.attrs.get("id").map(|s| s.as_str())
    }

    pub fn classes(&self) -> Vec<&str> {
        self.attrs
            .get("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default()
    }
}

/// A convenient builder for tests and fixtures.
pub struct NodeBuilder {
    node: Node,
}

impl NodeBuilder {
    pub fn element(tag: &str) -> Self {
        Self {
            node: Node::element(tag),
        }
    }
    pub fn text(s: &str) -> Self {
        Self {
            node: Node::text(s),
        }
    }
    pub fn attr(mut self, k: &str, v: &str) -> Self {
        if let Node::Element(e) = &mut self.node {
            e.attrs.insert(k.to_string(), v.to_string());
        }
        self
    }
    pub fn child(mut self, child: Node) -> Self {
        if let Node::Element(e) = &mut self.node {
            e.children.push(child);
        }
        self
    }
    pub fn build(self) -> Node {
        self.node
    }
}
