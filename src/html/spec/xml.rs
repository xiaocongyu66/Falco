//! XML / XHTML parser — strict error handling mode.
//!
//! Spec: https://www.w3.org/TR/xml/
//!
//! Unlike HTML5's permissive parser, XML is strict: a single well-formedness
//! error stops parsing. This module provides:
//!
//! * A streaming XML parser (not the HTML tokenizer in disguise)
//! * Namespace support (`xmlns:prefix="..."`)
//! * CDATA sections, processing instructions
//! * Strict attribute quoting (unquoted values are errors)
//! * Well-formedness checking (every start tag must have matching end tag)
//! * Entity references (only declared entities, plus the 5 predefined)
//!
//! XHTML documents (`Content-Type: application/xhtml+xml`) use this parser
//! instead of the HTML5 tree builder.

use crate::dom::spec::{append_child, set_attribute, Document, DocumentHandle, NodeRef};
use std::collections::HashMap;

/// XML parsing error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlError {
    UnexpectedEof,
    UnexpectedChar(char),
    MismatchedEndTag { expected: String, found: String },
    UnmatchedEndTag(String),
    UnclosedElement(String),
    DuplicateAttribute(String),
    UndeclaredEntity(String),
    InvalidAttributeSyntax,
    InvalidXmlDeclaration,
    InvalidNamespace,
}

/// Parse an XML / XHTML document into a DOM2 tree.
///
/// On any well-formedness error, returns the error and leaves the document
/// in a partial state (the tree built up to the point of failure).
pub fn parse(doc: DocumentHandle, src: &str) -> Result<(), XmlError> {
    let mut parser = XmlParser::new(doc, src);
    parser.parse_document()?;
    Ok(())
}

struct XmlParser<'a> {
    doc: DocumentHandle,
    input: &'a [u8],
    pos: usize,
    /// Stack of open elements (tag names).
    open: Vec<NodeRef>,
    /// Namespace prefix → URI map for the current scope.
    ns_stack: Vec<HashMap<String, String>>,
}

impl<'a> XmlParser<'a> {
    fn new(doc: DocumentHandle, src: &'a str) -> Self {
        Self {
            doc,
            input: src.as_bytes(),
            pos: 0,
            open: Vec::new(),
            ns_stack: vec![HashMap::new()],
        }
    }

    fn parse_document(&mut self) -> Result<(), XmlError> {
        self.skip_xml_decl()?;
        self.skip_misc()?;
        // Parse the root element.
        self.parse_element()?;
        self.skip_misc()?;
        if self.pos < self.input.len() {
            return Err(XmlError::UnexpectedChar(self.input[self.pos] as char));
        }
        Ok(())
    }

    fn skip_xml_decl(&mut self) -> Result<(), XmlError> {
        if self.starts_with("<?xml") {
            // Skip until "?>".
            while self.pos + 2 <= self.input.len() {
                if &self.input[self.pos..self.pos + 2] == b"?>" {
                    self.pos += 2;
                    return Ok(());
                }
                self.pos += 1;
            }
            return Err(XmlError::UnexpectedEof);
        }
        Ok(())
    }

    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts_with("<!--") {
                self.skip_comment()?;
            } else if self.starts_with("<?") {
                self.skip_pi()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_comment(&mut self) -> Result<(), XmlError> {
        self.pos += 4; // <!--
        while self.pos + 3 <= self.input.len() {
            if &self.input[self.pos..self.pos + 3] == b"-->" {
                self.pos += 3;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(XmlError::UnexpectedEof)
    }

    fn skip_pi(&mut self) -> Result<(), XmlError> {
        self.pos += 2; // <?
        while self.pos + 2 <= self.input.len() {
            if &self.input[self.pos..self.pos + 2] == b"?>" {
                self.pos += 2;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(XmlError::UnexpectedEof)
    }

    fn parse_element(&mut self) -> Result<(), XmlError> {
        if self.peek_byte() != Some(b'<') {
            return Err(XmlError::UnexpectedChar(
                self.peek_byte().unwrap_or(b'?') as char
            ));
        }
        self.pos += 1; // <
        let tag = self.parse_name()?;
        let tag = tag.to_lowercase();
        // Parse attributes.
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut ns_bindings: HashMap<String, String> = HashMap::new();
        loop {
            self.skip_ws();
            match self.peek_byte() {
                Some(b'/') => {
                    self.pos += 1;
                    if self.peek_byte() != Some(b'>') {
                        return Err(XmlError::InvalidAttributeSyntax);
                    }
                    self.pos += 1;
                    // Self-closing — create element and return.
                    let el = self.create_element_with_ns(&tag, &attrs, &ns_bindings);
                    let parent = self
                        .open
                        .last()
                        .cloned()
                        .unwrap_or_else(|| self.doc.borrow().root.clone());
                    append_child(&parent, el);
                    return Ok(());
                }
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => {
                    let attr_name = self.parse_name()?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b'=') {
                        return Err(XmlError::InvalidAttributeSyntax);
                    }
                    self.pos += 1;
                    self.skip_ws();
                    let value = self.parse_attr_value()?;
                    // Check for duplicate attributes.
                    if attrs.iter().any(|(n, _)| *n == attr_name) {
                        return Err(XmlError::DuplicateAttribute(attr_name));
                    }
                    // Handle xmlns bindings.
                    if attr_name == "xmlns" {
                        ns_bindings.insert(String::new(), value.clone());
                    } else if let Some(prefix) = attr_name.strip_prefix("xmlns:") {
                        ns_bindings.insert(prefix.to_string(), value.clone());
                    }
                    attrs.push((attr_name, value));
                }
                None => return Err(XmlError::UnexpectedEof),
            }
        }
        // Push ns scope.
        self.ns_stack.push(ns_bindings);
        let el = self.create_element_with_ns(&tag, &attrs, self.ns_stack.last().unwrap());
        let parent = self
            .open
            .last()
            .cloned()
            .unwrap_or_else(|| self.doc.borrow().root.clone());
        append_child(&parent, el.clone());
        self.open.push(el);
        // Parse content until matching end tag.
        loop {
            self.skip_ws();
            if self.pos >= self.input.len() {
                return Err(XmlError::UnclosedElement(tag.clone()));
            }
            if self.starts_with("</") {
                self.pos += 2;
                let end_tag = self.parse_name()?.to_lowercase();
                if end_tag != tag {
                    return Err(XmlError::MismatchedEndTag {
                        expected: tag.clone(),
                        found: end_tag,
                    });
                }
                self.skip_ws();
                if self.peek_byte() != Some(b'>') {
                    return Err(XmlError::InvalidAttributeSyntax);
                }
                self.pos += 1;
                self.open.pop();
                self.ns_stack.pop();
                return Ok(());
            }
            if self.starts_with("<!--") {
                self.skip_comment()?;
                continue;
            }
            if self.starts_with("<![CDATA[") {
                self.parse_cdata()?;
                continue;
            }
            if self.starts_with("<?") {
                self.skip_pi()?;
                continue;
            }
            if self.peek_byte() == Some(b'<') {
                self.parse_element()?;
                continue;
            }
            // Text content.
            self.parse_text()?;
        }
    }

    fn parse_cdata(&mut self) -> Result<(), XmlError> {
        self.pos += 9; // <![CDATA[
        let start = self.pos;
        while self.pos + 3 <= self.input.len() {
            if &self.input[self.pos..self.pos + 3] == b"]]>" {
                let data = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
                let text = Document::create_text(&self.doc, &data);
                let parent = self
                    .open
                    .last()
                    .cloned()
                    .unwrap_or_else(|| self.doc.borrow().root.clone());
                append_child(&parent, text);
                self.pos += 3;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(XmlError::UnexpectedEof)
    }

    fn parse_text(&mut self) -> Result<(), XmlError> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != b'<' {
            self.pos += 1;
        }
        if self.pos > start {
            let raw = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
            let decoded = decode_xml_entities(&raw)?;
            let text = Document::create_text(&self.doc, &decoded);
            let parent = self
                .open
                .last()
                .cloned()
                .unwrap_or_else(|| self.doc.borrow().root.clone());
            append_child(&parent, text);
        }
        Ok(())
    }

    fn parse_name(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.input[self.pos];
            if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b':' || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(XmlError::UnexpectedChar(
                self.peek_byte().unwrap_or(b'?') as char
            ));
        }
        Ok(String::from_utf8_lossy(&self.input[start..self.pos]).to_string())
    }

    fn parse_attr_value(&mut self) -> Result<String, XmlError> {
        let quote = self.peek_byte().ok_or(XmlError::UnexpectedEof)?;
        if quote != b'"' && quote != b'\'' {
            return Err(XmlError::InvalidAttributeSyntax);
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != quote {
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Err(XmlError::UnexpectedEof);
        }
        let raw = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
        self.pos += 1; // skip closing quote
        decode_xml_entities(&raw)
    }

    fn create_element_with_ns(
        &self,
        tag: &str,
        attrs: &[(String, String)],
        ns_bindings: &HashMap<String, String>,
    ) -> NodeRef {
        let el = Document::create_element(&self.doc, tag);
        for (k, v) in attrs {
            set_attribute(&el, k, v);
        }
        // Store namespace info via user_data (simpler than adding fields to ElementData).
        if let Some(default_ns) = ns_bindings.get("") {
            el.borrow_mut()
                .user_data
                .insert("__xml_default_ns".into(), default_ns.clone());
        }
        el
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        let b = s.as_bytes();
        self.pos + b.len() <= self.input.len() && &self.input[self.pos..self.pos + b.len()] == b
    }
}

/// Decode XML predefined entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`)
/// and numeric character references. XML does NOT support HTML named entities
/// beyond these five unless declared in a DTD.
fn decode_xml_entities(s: &str) -> Result<String, XmlError> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // Find ';'.
            let mut j = i + 1;
            while j < bytes.len() && j < i + 20 && bytes[j] != b';' {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] != b';' {
                return Err(XmlError::UndeclaredEntity(
                    String::from_utf8_lossy(&bytes[i..]).to_string(),
                ));
            }
            let entity = &s[i + 1..j];
            if let Some(rest) = entity.strip_prefix('#') {
                let code =
                    if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16)
                            .map_err(|_| XmlError::UndeclaredEntity(entity.to_string()))?
                    } else {
                        rest.parse::<u32>()
                            .map_err(|_| XmlError::UndeclaredEntity(entity.to_string()))?
                    };
                let c =
                    char::from_u32(code).ok_or(XmlError::UndeclaredEntity(entity.to_string()))?;
                out.push(c);
            } else {
                match entity {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    _ => return Err(XmlError::UndeclaredEntity(entity.to_string())),
                }
            }
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::tag_name;
    use crate::html::spec::serializer::serialize;

    #[test]
    fn parses_simple_xml() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        parse(doc.clone(), "<root><child>text</child></root>").unwrap();
        let ser = serialize(&root);
        assert!(ser.contains("<root>"));
        assert!(ser.contains("<child>"));
        assert!(ser.contains("text"));
    }

    #[test]
    fn parses_attributes() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        parse(doc.clone(), r#"<root attr="value" empty=""/>"#).unwrap();
        let ser = serialize(&root);
        assert!(ser.contains("attr=\"value\""));
    }

    #[test]
    fn rejects_mismatched_end_tag() {
        let doc = Document::create();
        let result = parse(doc, "<a><b></c></a>");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unclosed_element() {
        let doc = Document::create();
        let result = parse(doc, "<a><b>");
        assert!(result.is_err());
    }

    #[test]
    fn parses_cdata() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        parse(doc.clone(), "<root><![CDATA[<not a tag>]]></root>").unwrap();
        let ser = serialize(&root);
        assert!(ser.contains("&lt;not a tag&gt;"), "got: {}", ser);
    }

    #[test]
    fn parses_processing_instructions() {
        let doc = Document::create();
        let result = parse(doc, "<?xml version=\"1.0\"?><root/>");
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_unquoted_attributes() {
        let doc = Document::create();
        let result = parse(doc, "<root attr=value/>");
        assert!(result.is_err(), "unquoted attrs should be rejected in XML");
    }

    #[test]
    fn rejects_undeclared_entity() {
        let doc = Document::create();
        let result = parse(doc, "<root>&unknown;</root>");
        assert!(result.is_err());
    }

    #[test]
    fn decodes_predefined_entities() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        parse(doc.clone(), "<root>a &amp; b &lt; c &gt; d</root>").unwrap();
        let ser = serialize(&root);
        assert!(ser.contains("a &amp; b &lt; c &gt; d"), "got: {}", ser);
    }
}
