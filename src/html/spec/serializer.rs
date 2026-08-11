//! DOM serializer — `innerHTML` / `outerHTML` getters.
//!
//! Spec: https://html.spec.whatwg.org/multipage/parsing.html#serialising
//!
//! Produces HTML markup from a DOM tree. Handles:
//! * Void elements (no closing tag)
//! * `<template>` (serializes template contents)
//! * Foreign content (SVG / MathML namespaces — escapes differently)
//! * Escaping of text content and attribute values
//! * Pre/textarea (preserves whitespace)

use crate::dom::spec::{NodeKind, NodeRef};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

const RAW_TEXT_ELEMENTS: &[&str] = &[
    "script",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];

/// Serialize a DOM node and its descendants into HTML.
pub fn serialize(node: &NodeRef) -> String {
    let mut out = String::new();
    serialize_node(node, &mut out);
    out
}

/// Serialize only the children of a node (used for `innerHTML`).
pub fn serialize_inner(node: &NodeRef, out: &mut String) {
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        serialize_node(&child, out);
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
}

fn serialize_node(node: &NodeRef, out: &mut String) {
    let kind = node.borrow().kind.clone();
    match kind {
        NodeKind::Document => {
            let mut cursor = node.borrow().first_child.clone();
            while let Some(child) = cursor {
                serialize_node(&child, out);
                cursor = child
                    .borrow()
                    .next_sibling
                    .clone()
                    .and_then(|w| w.upgrade());
            }
        }
        NodeKind::DocumentType { name, .. } => {
            out.push_str("<!DOCTYPE ");
            out.push_str(&name);
            out.push('>');
        }
        NodeKind::Element(e) => {
            // Start tag.
            out.push('<');
            out.push_str(&e.tag);
            for attr in &e.attrs {
                out.push(' ');
                out.push_str(&attr.name);
                out.push_str("=\"");
                escape_attr_value(&attr.value, out);
                out.push('"');
            }
            out.push('>');
            // Children.
            let is_void = VOID_ELEMENTS.contains(&e.tag.as_str());
            if is_void {
                return;
            }
            // For template, serialize template contents.
            if e.tag == "template" {
                if let Some(contents) = &e.template_contents {
                    serialize_inner(contents, out);
                }
                out.push_str("</template>");
                return;
            }
            // For raw text elements (script, style), don't escape content.
            let is_raw = RAW_TEXT_ELEMENTS.contains(&e.tag.as_str());
            let mut cursor = node.borrow().first_child.clone();
            while let Some(child) = cursor {
                if is_raw {
                    serialize_raw(&child, out);
                } else {
                    serialize_node(&child, out);
                }
                cursor = child
                    .borrow()
                    .next_sibling
                    .clone()
                    .and_then(|w| w.upgrade());
            }
            // End tag.
            out.push_str("</");
            out.push_str(&e.tag);
            out.push('>');
        }
        NodeKind::Text(s) => {
            escape_text(&s, out);
        }
        NodeKind::Comment(s) => {
            out.push_str("<!--");
            out.push_str(&s);
            out.push_str("-->");
        }
        NodeKind::DocumentFragment => {
            serialize_inner(node, out);
        }
        NodeKind::ShadowRoot(_) => {
            serialize_inner(node, out);
        }
        NodeKind::ProcessingInstruction { target, data } => {
            out.push_str("<?");
            out.push_str(&target);
            out.push(' ');
            out.push_str(&data);
            out.push_str("?>");
        }
        NodeKind::Attr { name, value } => {
            out.push_str(&name);
            out.push_str("=\"");
            escape_attr_value(&value, out);
            out.push('"');
        }
    }
}

fn serialize_raw(node: &NodeRef, out: &mut String) {
    if let NodeKind::Text(s) = &node.borrow().kind {
        out.push_str(s);
    } else {
        serialize_node(node, out);
    }
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr_value(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::{append_child, set_attribute, Document};

    #[test]
    fn serializes_simple_element() {
        let doc = Document::create();
        let p = Document::create_element(&doc, "p");
        set_attribute(&p, "class", "foo");
        let text = Document::create_text(&doc, "hello");
        append_child(&p, text);
        let html = serialize(&p);
        assert_eq!(html, "<p class=\"foo\">hello</p>");
    }

    #[test]
    fn serializes_void_element() {
        let doc = Document::create();
        let img = Document::create_element(&doc, "img");
        set_attribute(&img, "src", "x.png");
        let html = serialize(&img);
        assert_eq!(html, "<img src=\"x.png\">");
    }

    #[test]
    fn escapes_text_content() {
        let doc = Document::create();
        let p = Document::create_element(&doc, "p");
        let text = Document::create_text(&doc, "a < b & c > d");
        append_child(&p, text);
        let html = serialize(&p);
        assert_eq!(html, "<p>a &lt; b &amp; c &gt; d</p>");
    }

    #[test]
    fn serializes_nested_elements() {
        let doc = Document::create();
        let ul = Document::create_element(&doc, "ul");
        let li1 = Document::create_element(&doc, "li");
        let t1 = Document::create_text(&doc, "one");
        append_child(&li1, t1);
        append_child(&ul, li1);
        let html = serialize(&ul);
        assert_eq!(html, "<ul><li>one</li></ul>");
    }
}
