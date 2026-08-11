//! HTML5 tokenizer and tree builder.
//!
//! This is NOT a full HTML5 spec implementation (the spec is ~100k lines).
//! It handles the common subset: tags with attributes, self-closing tags,
//! void elements, text content, comments, doctype, and entity references
//! for the most common entities. It does NOT handle:
//!   * Foster parenting (table repair)
//!   * Implicit `<tbody>` insertion
//!   * `<template>` content
//!   * Adoption agency algorithm (mis-nested `<b>/<i>` repair)
//!
//! These omissions are acceptable for our rendering target — the layout
//! engine handles block/inline flow and doesn't depend on HTML5's exotic
//! tree-repair rules.
//!
//! # Spec-compliant replacement
//!
//! The [`spec`] submodule contains the WHATWG-§13.2-compliant replacement
//! (all 80 tokenizer states, all 22 tree-builder insertion modes, adoption
//! agency algorithm, foster parenting, XML parser, encoding detection,
//! innerHTML/outerHTML serializer). It is **not yet wired into the render
//! pipeline** — `render_with_base_url` still uses the legacy `parse()`
//! function in this file.

pub mod spec;

use crate::dom::{DoctypeData, DocumentData, ElementData, Node, TextData};
use std::borrow::Cow;
use std::collections::HashMap;

const COMMON_ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{00A0}"),
    ("copy", "\u{00A9}"),
    ("reg", "\u{00AE}"),
    ("trade", "\u{2122}"),
    ("mdash", "\u{2014}"),
    ("ndash", "\u{2013}"),
    ("hellip", "\u{2026}"),
    ("laquo", "\u{00AB}"),
    ("raquo", "\u{00BB}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("euro", "\u{20AC}"),
    ("pound", "\u{00A3}"),
    ("yen", "\u{00A5}"),
    ("cent", "\u{00A2}"),
    ("sect", "\u{00A7}"),
    ("para", "\u{00B6}"),
    ("middot", "\u{00B7}"),
    ("bull", "\u{2022}"),
    ("deg", "\u{00B0}"),
    ("plusmn", "\u{00B1}"),
    ("times", "\u{00D7}"),
    ("divide", "\u{00F7}"),
    ("frac12", "\u{00BD}"),
    ("frac14", "\u{00BC}"),
    ("frac34", "\u{00BE}"),
    ("sup1", "\u{00B9}"),
    ("sup2", "\u{00B2}"),
    ("sup3", "\u{00B3}"),
    ("micro", "\u{00B5}"),
    ("alpha", "\u{03B1}"),
    ("beta", "\u{03B2}"),
    ("gamma", "\u{03B3}"),
    ("delta", "\u{03B4}"),
    ("Alpha", "\u{0391}"),
    ("Beta", "\u{0392}"),
    ("Gamma", "\u{0393}"),
    ("Delta", "\u{0394}"),
    ("pi", "\u{03C0}"),
    ("Pi", "\u{03A0}"),
    ("sigma", "\u{03C3}"),
    ("Sigma", "\u{03A3}"),
    ("omega", "\u{03C9}"),
    ("Omega", "\u{03A9}"),
    ("infin", "\u{221E}"),
    ("ne", "\u{2260}"),
    ("le", "\u{2264}"),
    ("ge", "\u{2265}"),
    ("larr", "\u{2190}"),
    ("rarr", "\u{2192}"),
    ("uarr", "\u{2191}"),
    ("darr", "\u{2193}"),
    ("harr", "\u{2194}"),
    ("lArr", "\u{21D0}"),
    ("rArr", "\u{21D2}"),
    ("uArr", "\u{21D1}"),
    ("dArr", "\u{21D3}"),
    ("hArr", "\u{21D4}"),
    ("spades", "\u{2660}"),
    ("clubs", "\u{2663}"),
    ("hearts", "\u{2665}"),
    ("diams", "\u{2666}"),
    ("check", "\u{2713}"),
    ("cross", "\u{2717}"),
    ("star", "\u{2605}"),
    ("sigma", "\u{03C3}"),
];

/// Token emitted by the tokenizer.
#[derive(Debug, Clone)]
enum Token {
    Doctype {
        name: String,
    },
    StartTag {
        name: String,
        attrs: HashMap<String, String>,
        self_closing: bool,
    },
    EndTag {
        name: String,
    },
    Text(String),
    Comment(String),
}

/// Tokenizer state machine. We keep this byte-oriented for speed.
struct Tokenizer<'a> {
    src: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Tokenizer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Token> {
        while self.pos < self.src.len() {
            if self.starts_with("<!--") {
                self.consume_comment();
            } else if self.starts_with("<!") {
                self.consume_doctype();
            } else if self.starts_with("</") {
                self.consume_end_tag();
            } else if self.current() == b'<' && self.peek_is_name(1) {
                self.consume_start_tag();
            } else {
                self.consume_text();
            }
        }
        self.tokens
    }

    fn current(&self) -> u8 {
        self.src[self.pos]
    }

    fn starts_with(&self, s: &str) -> bool {
        let bytes = s.as_bytes();
        self.pos + bytes.len() <= self.src.len()
            && &self.src[self.pos..self.pos + bytes.len()] == bytes
    }

    fn peek_is_name(&self, offset: usize) -> bool {
        let i = self.pos + offset;
        if i >= self.src.len() {
            return false;
        }
        let c = self.src[i];
        c.is_ascii_alphabetic() || c == b'_'
    }

    fn consume_comment(&mut self) {
        self.pos += 4; // skip "<!--"
        let start = self.pos;
        while self.pos + 3 <= self.src.len() && &self.src[self.pos..self.pos + 3] != b"-->" {
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(&self.src[start..self.pos]).to_string();
        if self.pos + 3 <= self.src.len() {
            self.pos += 3; // skip "-->"
        }
        self.tokens.push(Token::Comment(text));
    }

    fn consume_doctype(&mut self) {
        self.pos += 2; // skip "<!"
                       // Read until '>'
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] != b'>' {
            self.pos += 1;
        }
        let raw = String::from_utf8_lossy(&self.src[start..self.pos]).to_string();
        if self.pos < self.src.len() {
            self.pos += 1;
        } // skip '>'
          // Extract the name (first whitespace-separated word after "doctype").
        let name = raw
            .split_whitespace()
            .nth(1)
            .unwrap_or("html")
            .to_lowercase();
        self.tokens.push(Token::Doctype { name });
    }

    fn consume_end_tag(&mut self) {
        self.pos += 2; // skip "</"
        let name = self.consume_name();
        // Skip until '>'.
        while self.pos < self.src.len() && self.src[self.pos] != b'>' {
            self.pos += 1;
        }
        if self.pos < self.src.len() {
            self.pos += 1;
        }
        self.tokens.push(Token::EndTag {
            name: name.to_lowercase(),
        });
    }

    fn consume_start_tag(&mut self) {
        self.pos += 1; // skip '<'
        let name = self.consume_name().to_lowercase();
        let mut attrs = HashMap::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            if self.pos >= self.src.len() {
                break;
            }
            let c = self.src[self.pos];
            if c == b'>' {
                self.pos += 1;
                break;
            }
            if c == b'/' {
                // Could be self-closing: "/>"
                if self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'>' {
                    self_closing = true;
                    self.pos += 2;
                    break;
                }
                self.pos += 1;
                continue;
            }
            // Attribute name.
            let attr_name = self.consume_attr_name();
            if attr_name.is_empty() {
                // Stuck — skip a byte to make progress.
                self.pos += 1;
                continue;
            }
            self.skip_whitespace();
            let mut value = String::new();
            if self.pos < self.src.len() && self.src[self.pos] == b'=' {
                self.pos += 1;
                self.skip_whitespace();
                value = self.consume_attr_value();
            }
            let attr_name_lower = attr_name.to_lowercase();
            let decoded_value = decode_entities(&value);
            // Security: filter unsafe attributes using CSP is_safe_attribute().
            // This blocks onclick/onload handlers and javascript: URLs in
            // href/src/action. The check is applied at parse time so the
            // dangerous attributes never enter the DOM.
            if crate::security::csp::is_safe_attribute(&name, &attr_name_lower, &decoded_value) {
                attrs.insert(attr_name_lower, decoded_value);
            } else {
                eprintln!(
                    "[falco:csp] blocked unsafe attribute: {}={} on <{}>",
                    attr_name_lower, decoded_value, name
                );
            }
        }
        self.tokens.push(Token::StartTag {
            name,
            attrs,
            self_closing,
        });
    }

    fn consume_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).to_string()
    }

    fn consume_attr_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c == b'=' || c == b'>' || c == b'/' || c.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).to_string()
    }

    fn consume_attr_value(&mut self) -> String {
        if self.pos >= self.src.len() {
            return String::new();
        }
        let quote = self.src[self.pos];
        if quote == b'"' || quote == b'\'' {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos] != quote {
                self.pos += 1;
            }
            let val = String::from_utf8_lossy(&self.src[start..self.pos]).to_string();
            if self.pos < self.src.len() {
                self.pos += 1;
            }
            val
        } else {
            // Unquoted value — read until whitespace or '>'.
            let start = self.pos;
            while self.pos < self.src.len() {
                let c = self.src[self.pos];
                if c.is_ascii_whitespace() || c == b'>' {
                    break;
                }
                self.pos += 1;
            }
            String::from_utf8_lossy(&self.src[start..self.pos]).to_string()
        }
    }

    fn consume_text(&mut self) {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c == b'<' {
                break;
            }
            self.pos += 1;
        }
        if self.pos > start {
            let raw = String::from_utf8_lossy(&self.src[start..self.pos]).to_string();
            self.tokens.push(Token::Text(decode_entities(&raw)));
        } else {
            // Stray '<' — consume it as text to avoid infinite loop.
            self.pos += 1;
            self.tokens.push(Token::Text("<".into()));
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }
}

/// Decode HTML entity references (`&amp;`, `&#65;`, `&copy;`, etc.).
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // Find ';'
            let mut j = i + 1;
            while j < bytes.len()
                && j < i + 12
                && bytes[j] != b';'
                && !bytes[j].is_ascii_whitespace()
            {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b';' && j > i + 1 {
                let entity = &s[i + 1..j];
                if let Some(decoded) = decode_one(entity) {
                    out.push_str(&decoded);
                    i = j + 1;
                    continue;
                }
            }
            // Not a valid entity — emit '&' literally.
            out.push('&');
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn decode_one(entity: &str) -> Option<Cow<'static, str>> {
    if let Some(rest) = entity.strip_prefix('#') {
        // Numeric entity.
        let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            rest.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(|c| Cow::Owned(c.to_string()));
    }
    COMMON_ENTITIES
        .iter()
        .find(|(name, _)| *name == entity)
        .map(|(_, val)| Cow::Borrowed(*val))
}

/// Parse an HTML source string into a DOM tree rooted at Document.
pub fn parse(src: &str) -> Node {
    let tokens = Tokenizer::new(src).run();
    let mut builder = TreeBuilder::new();
    for tok in tokens {
        builder.consume(tok);
    }
    builder.finish()
}

/// Tree builder — turns a flat token stream into a DOM tree.
struct TreeBuilder {
    /// Flat storage of all nodes. Children are stored as indices into this Vec.
    /// This is much faster than `Rc<RefCell<>>` and avoids borrow-checker pain.
    /// The Document root is always at index 0.
    nodes: Vec<Node>,
    /// Stack of open element indices. The last entry is the current insertion target.
    stack: Vec<usize>,
    /// Map from parent_idx -> child indices. Built up as nodes are added.
    children: std::collections::HashMap<usize, Vec<usize>>,
}

impl TreeBuilder {
    fn new() -> Self {
        let mut nodes = Vec::new();
        nodes.push(Node::Document(DocumentData {
            children: Vec::new(),
        }));
        Self {
            nodes,
            stack: vec![0],
            children: std::collections::HashMap::new(),
        }
    }

    fn current_idx(&self) -> usize {
        *self.stack.last().unwrap()
    }

    fn consume(&mut self, tok: Token) {
        match tok {
            Token::Doctype { name } => {
                let node = Node::Doctype(DoctypeData { name });
                self.append_child(node);
            }
            Token::Comment(text) => {
                self.append_child(Node::Comment(text));
            }
            Token::Text(text) => {
                // Collapse whitespace runs into a single space — matches
                // how browsers render text in non-`<pre>` elements.
                let collapsed = collapse_whitespace(&text);
                if !collapsed.is_empty() {
                    self.append_child(Node::Text(TextData { text: collapsed }));
                }
            }
            Token::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                if Node::is_void_tag(&name) || self_closing {
                    let node = Node::Element(ElementData {
                        tag: name,
                        attrs,
                        children: Vec::new(),
                    });
                    self.append_child(node);
                    return;
                }
                // Auto-close common tags before pushing.
                self.maybe_auto_close(&name);
                let node = Node::Element(ElementData {
                    tag: name,
                    attrs,
                    children: Vec::new(),
                });
                let idx = self.nodes.len();
                self.nodes.push(node);
                let parent = self.current_idx();
                self.children.entry(parent).or_default().push(idx);
                self.stack.push(idx);
            }
            Token::EndTag { name } => {
                // Pop until we find a matching open tag. If not found, ignore.
                let mut found = None;
                for (i, idx) in self.stack.iter().enumerate().rev() {
                    if let Node::Element(e) = &self.nodes[*idx] {
                        if e.tag == name {
                            found = Some(i);
                            break;
                        }
                    }
                }
                if let Some(i) = found {
                    self.stack.truncate(i);
                }
            }
        }
    }

    fn append_child(&mut self, node: Node) {
        let idx = self.nodes.len();
        self.nodes.push(node);
        let parent = self.current_idx();
        self.children.entry(parent).or_default().push(idx);
    }

    /// Implement common HTML auto-closing rules: `<li>` closes a previous `<li>`,
    /// `<p>` closes a previous `<p>`, `<td>` closes a previous `<td>`, etc.
    fn maybe_auto_close(&mut self, new_tag: &str) {
        let close_when: &[&str] = match new_tag {
            "li" => &["li"],
            "p" | "address" | "blockquote" | "center" => &["p"],
            "td" | "th" => &["td", "th"],
            "tr" => &["tr", "td", "th"],
            "thead" | "tbody" | "tfoot" => &["thead", "tbody", "tfoot", "td", "th", "tr"],
            "option" => &["option"],
            "dt" | "dd" => &["dt", "dd"],
            _ => &[],
        };
        if close_when.is_empty() {
            return;
        }
        // Find the closest matching open tag.
        let mut found = None;
        for (i, idx) in self.stack.iter().enumerate().rev() {
            if let Node::Element(e) = &self.nodes[*idx] {
                if close_when.contains(&e.tag.as_str()) {
                    found = Some(i);
                    break;
                }
                // Stop at block boundaries (don't close across block elements).
                if matches!(
                    e.tag.as_str(),
                    "ul" | "ol" | "table" | "div" | "section" | "article"
                ) {
                    break;
                }
            }
        }
        if let Some(i) = found {
            self.stack.truncate(i);
        }
    }

    fn finish(mut self) -> Node {
        // Assemble the final tree by recursing from the root.
        self.assemble(0)
    }

    fn assemble(&mut self, idx: usize) -> Node {
        let child_indices = self.children.remove(&idx).unwrap_or_default();
        let children: Vec<Node> = child_indices
            .into_iter()
            .map(|c| self.assemble(c))
            .collect();
        // Take the node out of storage (we replace it with a placeholder).
        let placeholder = Node::Comment("".into());
        let mut node = std::mem::replace(&mut self.nodes[idx], placeholder);
        match &mut node {
            Node::Document(d) => d.children = children,
            Node::Element(e) => e.children = children,
            _ => {}
        }
        node
    }
}

/// Collapse runs of whitespace into a single space, matching browser behavior
/// for inline text. Newlines and tabs become single spaces.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_document() {
        let html =
            r#"<!DOCTYPE html><html><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>"#;
        let dom = parse(html);
        assert!(matches!(dom, Node::Document(_)));
    }

    #[test]
    fn debug_parse() {
        let html = "<p>line1<br>line2<img src=\"x.png\"></p>";
        let dom = parse(html);
        if let Node::Document(d) = dom {
            println!("Document has {} children", d.children.len());
            for (i, c) in d.children.iter().enumerate() {
                println!("  child {i}: {:?}", c.as_element().map(|e| e.tag.as_str()));
            }
        }
    }

    #[test]
    fn handles_void_tags() {
        let html = "<p>line1<br>line2<img src=\"x.png\"></p>";
        let dom = parse(html);
        if let Node::Document(d) = dom {
            println!("Document has {} children", d.children.len());
            // <html> implicit? No — we don't auto-insert html/body. Just <p>.
            // Walk to find <p>.
            let p = find_first_tag(&d.children, "p");
            assert!(p.is_some());
            let p = p.unwrap();
            // <p> should contain Text, br, Text, img.
            assert_eq!(p.children.len(), 4);
        }
    }

    #[test]
    fn decodes_entities() {
        let html = "<p>a &amp; b &lt; c &copy; &#65;</p>";
        let dom = parse(html);
        if let Node::Document(d) = dom {
            let p = find_first_tag(&d.children, "p").unwrap();
            let text: String = p
                .children
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                .collect();
            assert_eq!(text, "a & b < c © A");
        }
    }

    #[test]
    fn auto_closes_li() {
        let html = "<ul><li>one<li>two<li>three</ul>";
        let dom = parse(html);
        if let Node::Document(d) = dom {
            let ul = find_first_tag(&d.children, "ul").unwrap();
            assert_eq!(ul.children.len(), 3);
            for c in &ul.children {
                assert!(c.as_element().unwrap().tag == "li");
            }
        }
    }

    #[test]
    fn csp_blocks_onclick_attribute() {
        let html = r#"<div onclick="alert('xss')">text</div>"#;
        let dom = parse(html);
        let children = match &dom {
            Node::Document(d) => &d.children,
            _ => panic!("expected Document"),
        };
        let div = find_first_tag(children, "div").unwrap();
        // onclick should be filtered out by CSP is_safe_attribute.
        assert!(!div.attrs.contains_key("onclick"));
    }

    #[test]
    fn csp_blocks_javascript_url() {
        let html = r#"<a href="javascript:alert(1)">link</a>"#;
        let dom = parse(html);
        let children = match &dom {
            Node::Document(d) => &d.children,
            _ => panic!("expected Document"),
        };
        let a = find_first_tag(children, "a").unwrap();
        // javascript: URL should be filtered out.
        assert!(!a.attrs.contains_key("href"));
    }

    #[test]
    fn csp_allows_safe_attributes() {
        let html = r#"<a href="https://example.com" class="link">link</a>"#;
        let dom = parse(html);
        let children = match &dom {
            Node::Document(d) => &d.children,
            _ => panic!("expected Document"),
        };
        let a = find_first_tag(children, "a").unwrap();
        assert_eq!(a.attrs.get("href").unwrap(), "https://example.com");
        assert_eq!(a.attrs.get("class").unwrap(), "link");
    }

    fn find_first_tag<'a>(nodes: &'a [Node], tag: &str) -> Option<&'a ElementData> {
        for n in nodes {
            if let Node::Element(e) = n {
                if e.tag == tag {
                    return Some(e);
                }
                if let Some(found) = find_first_tag(&e.children, tag) {
                    return Some(found);
                }
            }
        }
        None
    }
}
