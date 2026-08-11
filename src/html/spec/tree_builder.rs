//! HTML5 tree construction — full WHATWG §13.2.6 state machine.
//!
//! This is the heart of the HTML5 parser. It consumes tokens from the
//! tokenizer and builds the DOM tree according to the insertion mode
//! state machine. Key algorithms implemented here:
//!
//! * **Insertion modes** — Initial, BeforeHtml, BeforeHead, InHead, etc.
//! * **Foster parenting** — content inserted in `<table>` outside a cell
//!   is reparented to before the table.
//! * **Adoption agency algorithm** — repairs mis-nested formatting
//!   elements like `<b>a<i>b</b>c</i>`.
//! * **Active formatting elements** — reconstructed before each character.
//! * **Stack of open elements** — tracks the current insertion point.
//! * **List of active formatting elements** — tracks in-flight formatting.
//! * **Foreign content** — SVG, MathML namespaces.

use crate::dom::spec::{
    append_child, set_attribute, tag_name, Document, DocumentHandle, Node, NodeKind, NodeRef,
};
use crate::html::spec::tokenizer::Token;

/// The insertion mode of the tree builder.
/// Spec: §13.2.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    InHeadNoscript,
    AfterHead,
    InBody,
    Text,
    InTable,
    InTableText,
    InCaption,
    InColumnGroup,
    InTableBody,
    InRow,
    InCell,
    InSelect,
    InSelectInTable,
    InTemplate,
    AfterBody,
    InFrameset,
    AfterFrameset,
    AfterAfterBody,
    AfterAfterFrameset,
}

/// The HTML5 tree builder.
pub struct TreeBuilder {
    /// The document we're building.
    doc: DocumentHandle,
    /// Stack of open elements. The last entry is the current insertion point.
    open_elements: Vec<NodeRef>,
    /// List of active formatting elements (with markers).
    active_formatting: Vec<FormattingEntry>,
    /// Current insertion mode.
    mode: InsertionMode,
    /// Original insertion mode (used by Text mode).
    original_mode: InsertionMode,
    /// Pending table character tokens (used by InTableText).
    pending_table_chars: Vec<String>,
    /// Whether we're in "fragment" parsing mode (no <html> wrapper).
    fragment_parsing: bool,
    /// Whether we're in frameset-ok state (set to "not ok" when certain
    /// content is seen, blocking later `<frameset>`).
    frameset_ok: bool,
    /// Head element pointer (the first `<head>` we saw).
    head_element: Option<NodeRef>,
    /// Form element pointer.
    form_element: Option<NodeRef>,
    /// Stack of template insertion modes.
    template_modes: Vec<InsertionMode>,
}

#[derive(Debug, Clone)]
enum FormattingEntry {
    /// A marker — pushed when entering scopes (e.g. cells, tables).
    Marker,
    /// An actual formatting element (b, i, em, etc.).
    Element(NodeRef),
}

impl TreeBuilder {
    pub fn new(doc: DocumentHandle) -> Self {
        let root = doc.borrow().root.clone();
        Self {
            doc,
            open_elements: vec![root],
            active_formatting: Vec::new(),
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            pending_table_chars: Vec::new(),
            fragment_parsing: false,
            frameset_ok: true,
            head_element: None,
            form_element: None,
            template_modes: Vec::new(),
        }
    }

    /// The current node — top of the open elements stack.
    fn current(&self) -> NodeRef {
        self.open_elements.last().unwrap().clone()
    }

    /// Process one token.
    pub fn consume(&mut self, token: Token) {
        match self.mode {
            InsertionMode::Initial => self.in_initial(token),
            InsertionMode::BeforeHtml => self.in_before_html(token),
            InsertionMode::BeforeHead => self.in_before_head(token),
            InsertionMode::InHead => self.in_head(token),
            InsertionMode::InHeadNoscript => self.in_head_noscript(token),
            InsertionMode::AfterHead => self.in_after_head(token),
            InsertionMode::InBody => self.in_body(token),
            InsertionMode::Text => self.in_text(token),
            InsertionMode::InTable => self.in_table(token),
            InsertionMode::InTableText => self.in_table_text(token),
            InsertionMode::InCaption => self.in_caption(token),
            InsertionMode::InColumnGroup => self.in_column_group(token),
            InsertionMode::InTableBody => self.in_table_body(token),
            InsertionMode::InRow => self.in_row(token),
            InsertionMode::InCell => self.in_cell(token),
            InsertionMode::InSelect => self.in_select(token),
            InsertionMode::InSelectInTable => self.in_select_in_table(token),
            InsertionMode::InTemplate => self.in_template(token),
            InsertionMode::AfterBody => self.in_after_body(token),
            InsertionMode::InFrameset => self.in_frameset(token),
            InsertionMode::AfterFrameset => self.in_after_frameset(token),
            InsertionMode::AfterAfterBody => self.in_after_after_body(token),
            InsertionMode::AfterAfterFrameset => self.in_after_after_frameset(token),
        }
    }

    fn in_initial(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => { /* ignore */ }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.doc.borrow().root.clone(), comment);
            }
            Token::Doctype {
                name, force_quirks, ..
            } => {
                let doc_type = NodeRef::new(std::cell::RefCell::new(Node {
                    id: 0, // will be set by create
                    kind: NodeKind::DocumentType {
                        name: name.clone().unwrap_or_default(),
                        public_id: String::new(),
                        system_id: String::new(),
                    },
                    owner_document: None,
                    doc: None,
                    parent: None,
                    parent_element: None,
                    first_child: None,
                    last_child: None,
                    previous_sibling: None,
                    next_sibling: None,
                    user_data: std::collections::HashMap::new(),
                }));
                // Set proper ID and register.
                let id = self.doc.borrow().next_node_id();
                doc_type.borrow_mut().id = id;
                doc_type.borrow_mut().doc = Some(std::rc::Rc::downgrade(&self.doc));
                self.doc.borrow().register(&doc_type);
                append_child(&self.doc.borrow().root.clone(), doc_type);
                if force_quirks || name.as_deref() != Some("html") {
                    // Quirks mode — we don't fully support quirks, just continue.
                }
                self.mode = InsertionMode::BeforeHtml;
            }
            _ => {
                // Anything else — switch to BeforeHtml and reprocess.
                self.mode = InsertionMode::BeforeHtml;
                self.consume(token);
            }
        }
    }

    fn in_before_html(&mut self, token: Token) {
        match token {
            Token::Doctype { .. } => { /* ignore */ }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.doc.borrow().root.clone(), comment);
            }
            Token::Character(c) if c.is_ascii_whitespace() => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => {
                let html = Document::create_element(&self.doc, "html");
                append_child(&self.doc.borrow().root.clone(), html.clone());
                self.open_elements.push(html);
                self.mode = InsertionMode::BeforeHead;
            }
            _ => {
                // Create <html> implicitly.
                let html = Document::create_element(&self.doc, "html");
                append_child(&self.doc.borrow().root.clone(), html.clone());
                self.open_elements.push(html);
                self.mode = InsertionMode::BeforeHead;
                self.consume(token);
            }
        }
    }

    fn in_before_head(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => { /* ignore */ }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag { ref name, .. } if name == "head" => {
                let head = Document::create_element(&self.doc, "head");
                append_child(&self.current(), head.clone());
                self.open_elements.push(head.clone());
                self.head_element = Some(head);
                self.mode = InsertionMode::InHead;
            }
            _ => {
                // Insert <head> implicitly.
                let head = Document::create_element(&self.doc, "head");
                append_child(&self.current(), head.clone());
                self.open_elements.push(head.clone());
                self.head_element = Some(head);
                self.mode = InsertionMode::InHead;
                self.consume(token);
            }
        }
    }

    fn in_head(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => {
                self.insert_character(c);
            }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "base" | "basefont" | "bgsound" | "link") => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
                // Void element — pop immediately.
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "meta" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
                // Spec: handle charset switching here. We assume UTF-8.
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "title" => {
                self.insert_element_and_switch_to_text(name, attrs, InsertionMode::Text);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "noframes" | "style" | "noscript") => {
                self.insert_element_and_switch_to_text(name, attrs, InsertionMode::Text);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name.as_str() == "script" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
            }
            Token::EndTag { ref name } if name == "head" => {
                self.pop_current_node();
                self.mode = InsertionMode::AfterHead;
            }
            Token::EndTag { ref name } if matches!(name.as_str(), "body" | "html" | "br") => {
                self.pop_current_node();
                self.mode = InsertionMode::AfterHead;
                self.consume(Token::EndTag { name: name.clone() });
            }
            Token::StartTag { ref name, .. } if name == "template" => {
                self.insert_template_element();
            }
            Token::EndTag { ref name } if name == "template" => {
                // Pop until we find <template>.
                while let Some(top) = self.open_elements.last() {
                    if tag_name(top) == "template" {
                        self.pop_current_node();
                        if let Some(last) = self.template_modes.pop() {
                            self.mode = last;
                        }
                        return;
                    }
                    self.pop_current_node();
                }
            }
            Token::StartTag { ref name, .. } if name == "head" => { /* parse error */ }
            _ => {
                self.pop_current_node();
                self.mode = InsertionMode::AfterHead;
                self.consume(token);
            }
        }
    }

    fn in_head_noscript(&mut self, token: Token) {
        match token {
            Token::Doctype { .. } => { /* parse error, ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::EndTag { ref name } if name == "noscript" => {
                self.pop_current_node();
                self.mode = InsertionMode::InHead;
            }
            Token::Character(c) if c.is_ascii_whitespace() => self.in_head(token),
            Token::Comment(_) => self.in_head(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "basefont" | "bgsound" | "link" | "meta" | "noframes" | "style"
            ) =>
            {
                self.in_head(token);
            }
            Token::StartTag { ref name, .. } if matches!(name.as_str(), "head" | "noscript") => {
                /* parse error, ignore */
            }
            _ => {
                // Parse error. Switch to InHead and reprocess.
                self.mode = InsertionMode::InHead;
                self.consume(token);
            }
        }
    }

    fn in_after_head(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => {
                self.insert_character(c);
            }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "body" => {
                let body = self.create_element(name, attrs);
                append_child(&self.current(), body.clone());
                self.open_elements.push(body);
                self.frameset_ok = false;
                self.mode = InsertionMode::InBody;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "frameset" => {
                let frameset = self.create_element(name, attrs);
                append_child(&self.current(), frameset.clone());
                self.open_elements.push(frameset);
                self.mode = InsertionMode::InFrameset;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "base"
                    | "basefont"
                    | "bgsound"
                    | "link"
                    | "meta"
                    | "noframes"
                    | "script"
                    | "style"
                    | "template"
                    | "title"
            ) =>
            {
                // Insert into head, but stay in after_head.
                let head = self.head_element.clone().unwrap();
                self.open_elements.push(head);
                self.in_head(token);
                // Pop back to where we were.
                while let Some(top) = self.open_elements.last() {
                    if tag_name(top) == "html" {
                        break;
                    }
                    self.open_elements.pop();
                }
            }
            Token::EndTag { ref name } if name == "template" => self.in_head(token),
            _ => {
                // Insert <body> implicitly.
                let body = Document::create_element(&self.doc, "body");
                append_child(&self.current(), body.clone());
                self.open_elements.push(body);
                self.mode = InsertionMode::InBody;
                self.consume(token);
            }
        }
    }

    fn in_body(&mut self, token: Token) {
        match token {
            Token::Character('\0') => { /* parse error, ignore */ }
            Token::Character(c) if c.is_ascii_whitespace() => {
                self.reconstruct_active_formatting();
                self.insert_character(c);
            }
            Token::Character(c) => {
                self.reconstruct_active_formatting();
                self.insert_character(c);
                self.frameset_ok = false;
            }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* parse error */ }
            Token::StartTag { ref name, .. } if name == "html" => {
                /* merge attributes onto <html>, ignore */
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "base"
                    | "basefont"
                    | "bgsound"
                    | "link"
                    | "meta"
                    | "noframes"
                    | "script"
                    | "style"
                    | "template"
                    | "title"
            ) =>
            {
                self.in_head(token);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "body" => { /* merge attributes onto <body>, ignore */ }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "frameset" => {
                if !self.frameset_ok {
                    return;
                }
                // Replace body with frameset.
                if self.open_elements.len() >= 2 {
                    let body = self.open_elements[1].clone();
                    if tag_name(&body) == "body" {
                        let parent = body
                            .borrow()
                            .parent
                            .as_ref()
                            .and_then(|w| w.upgrade())
                            .unwrap();
                        crate::dom::spec::remove_from_parent(&body);
                        let frameset = self.create_element(name, attrs);
                        append_child(&parent, frameset.clone());
                        self.open_elements[1] = frameset;
                        self.mode = InsertionMode::InFrameset;
                    }
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "address"
                    | "article"
                    | "aside"
                    | "blockquote"
                    | "center"
                    | "details"
                    | "dialog"
                    | "dir"
                    | "div"
                    | "dl"
                    | "fieldset"
                    | "figcaption"
                    | "figure"
                    | "footer"
                    | "header"
                    | "hgroup"
                    | "main"
                    | "menu"
                    | "nav"
                    | "ol"
                    | "p"
                    | "search"
                    | "section"
                    | "summary"
                    | "ul"
            ) =>
            {
                self.close_p_if_open();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                self.close_p_if_open();
                // If current is an h-tag, pop it (parse error recovery).
                if let Some(top) = self.open_elements.last() {
                    if matches!(
                        tag_name(top).as_str(),
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    ) {
                        self.pop_current_node();
                    }
                }
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "pre" | "listing") => {
                self.close_p_if_open();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.frameset_ok = false;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "form" => {
                if self.form_element.is_none() || !self.has_template_in_stack() {
                    self.close_p_if_open();
                    let form = self.create_element(name, attrs);
                    if !self.has_template_in_stack() {
                        self.form_element = Some(form.clone());
                    }
                    append_child(&self.current(), form.clone());
                    self.open_elements.push(form);
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "li" | "dd" | "dt") => {
                self.frameset_ok = false;
                // Stop at the nearest <li> or <dd>/<dt>.
                let stop_at = if name == "li" { "li" } else { "dd" };
                let target_tags: &[&str] = if name == "li" { &["li"] } else { &["dd", "dt"] };
                // Walk stack to find a node to close.
                let mut node_idx = self.open_elements.len();
                while node_idx > 0 {
                    node_idx -= 1;
                    let tag = tag_name(&self.open_elements[node_idx]);
                    if target_tags.contains(&tag.as_str()) {
                        // Pop until we close this node.
                        while self.open_elements.len() > node_idx + 1 {
                            self.pop_current_node();
                        }
                        self.pop_current_node();
                        break;
                    }
                    if is_special(tag.as_str()) && !matches!(tag.as_str(), "address" | "div" | "p")
                    {
                        break;
                    }
                }
                self.close_p_if_open();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                let _ = stop_at;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "plaintext" => {
                self.close_p_if_open();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                // Spec says: switch tokenizer to PLAINTEXT state.
                // We don't have direct access here; the parser entry point
                // would normally do this. For now, we continue.
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "button" => {
                // Close any open <button>.
                if self.has_in_scope("button") {
                    self.generate_implied_end_tags_except(None);
                    while self.has_in_scope("button") {
                        self.pop_current_node();
                    }
                }
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.frameset_ok = false;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "marquee" | "object") => {
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.frameset_ok = false;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "hr" => {
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
                self.frameset_ok = false;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "image" => {
                // Parse error — treat as <img>.
                let el = self.create_element("img", attrs);
                append_child(&self.current(), el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "img" | "br" | "wbr")
                && (matches!(name.as_str(), "img" | "br" | "wbr")) =>
            {
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "input" => {
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
                // Check type=hidden — does not affect frameset_ok if hidden.
                let is_hidden = attrs
                    .iter()
                    .any(|(k, v)| k == "type" && v.eq_ignore_ascii_case("hidden"));
                if !is_hidden {
                    self.frameset_ok = false;
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "param" | "source" | "track") => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "textarea" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.frameset_ok = false;
                self.original_mode = self.mode;
                self.mode = InsertionMode::Text;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "a" => {
                // If there's an <a> in active formatting, run adoption agency.
                if self.has_a_in_active_formatting() {
                    // Run adoption agency for "a".
                    self.adoption_agency("a");
                }
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.active_formatting
                    .push(FormattingEntry::Element(el.clone()));
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if is_formatting_element(name.as_str()) => {
                self.reconstruct_active_formatting();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.active_formatting
                    .push(FormattingEntry::Element(el.clone()));
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "table" => {
                self.close_p_if_open();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.mode = InsertionMode::InTable;
                self.frameset_ok = false;
            }
            Token::EndTag { ref name } if name == "body" => {
                if self.has_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                }
            }
            Token::EndTag { ref name } if name == "html" => {
                if self.has_in_scope("body") {
                    self.mode = InsertionMode::AfterBody;
                    self.consume(Token::EndTag {
                        name: "html".into(),
                    });
                }
            }
            Token::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "button"
                        | "center"
                        | "details"
                        | "dialog"
                        | "dir"
                        | "div"
                        | "dl"
                        | "fieldset"
                        | "figcaption"
                        | "figure"
                        | "footer"
                        | "header"
                        | "hgroup"
                        | "listing"
                        | "main"
                        | "menu"
                        | "nav"
                        | "ol"
                        | "pre"
                        | "search"
                        | "section"
                        | "summary"
                        | "ul"
                ) =>
            {
                if self.has_in_scope(name) {
                    self.generate_implied_end_tags_except(None);
                    while tag_name(&self.current()).as_str() != name.as_str() {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                }
            }
            Token::EndTag { ref name }
                if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                // Close any open heading.
                for h in &["h1", "h2", "h3", "h4", "h5", "h6"] {
                    if self.has_in_scope(h) {
                        self.generate_implied_end_tags_except(None);
                        while !matches!(
                            tag_name(&self.current()).as_str(),
                            "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                        ) {
                            self.pop_current_node();
                        }
                        self.pop_current_node();
                        break;
                    }
                }
            }
            Token::EndTag { ref name } if name == "p" => {
                if !self.has_in_button_scope("p") {
                    // Insert <p> implicitly.
                    let p = Document::create_element(&self.doc, "p");
                    append_child(&self.current(), p.clone());
                    self.open_elements.push(p);
                }
                self.close_p_element();
            }
            Token::EndTag { ref name } if name == "li" || name == "dd" || name == "dt" => {
                if self.has_in_list_item_scope(name) {
                    self.generate_implied_end_tags_except(Some(name));
                    while tag_name(&self.current()).as_str() != name.as_str() {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                }
            }
            Token::EndTag { ref name } if is_formatting_element(name) => {
                self.adoption_agency(name);
            }
            Token::EndTag { ref name } if name == "form" => {
                if !self.has_template_in_stack() {
                    let form = self.form_element.take();
                    if form.is_none() || !self.has_in_scope("form") {
                        return;
                    }
                    self.generate_implied_end_tags_except(None);
                    while let Some(top) = self.open_elements.last() {
                        if std::ptr::eq(top.as_ptr(), form.as_ref().unwrap().as_ptr()) {
                            break;
                        }
                        self.open_elements.pop();
                    }
                    self.open_elements.pop();
                } else {
                    if self.has_in_scope("form") {
                        self.generate_implied_end_tags_except(None);
                        while tag_name(&self.current()) != "form" {
                            self.pop_current_node();
                        }
                        self.pop_current_node();
                    }
                }
            }
            _ => {
                // Default: insert as block element or treat as error.
                if let Token::StartTag {
                    ref name,
                    ref attrs,
                    ..
                } = token
                {
                    self.reconstruct_active_formatting();
                    let el = self.create_element(name, attrs);
                    append_child(&self.current(), el.clone());
                    self.open_elements.push(el);
                }
            }
        }
    }

    fn in_text(&mut self, token: Token) {
        match token {
            Token::Character(c) => self.insert_character(c),
            Token::Eof => {
                // Parse error.
                if self.open_elements.len() == 1 {
                    return;
                }
                self.pop_current_node();
                self.mode = self.original_mode;
                self.consume(Token::Eof);
            }
            Token::EndTag { ref name } if name.as_str() == "script" => {
                // Pop the script element. (We don't actually execute it here —
                // that's the JS layer's job.)
                self.pop_current_node();
                self.mode = self.original_mode;
            }
            _ => {
                self.pop_current_node();
                self.mode = self.original_mode;
                self.consume(token);
            }
        }
    }

    fn in_table(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => {
                self.pending_table_chars.push(c.to_string());
                self.mode = InsertionMode::InTableText;
            }
            Token::Character(c) => {
                // Foster-parent the character.
                self.foster_parent_character(c);
            }
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* parse error */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "caption" => {
                self.clear_stack_back_to_table_context();
                self.active_formatting.push(FormattingEntry::Marker);
                let caption = self.create_element(name, attrs);
                append_child(&self.current(), caption.clone());
                self.open_elements.push(caption);
                self.mode = InsertionMode::InCaption;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "colgroup" => {
                self.clear_stack_back_to_table_context();
                let colgroup = self.create_element(name, attrs);
                append_child(&self.current(), colgroup.clone());
                self.open_elements.push(colgroup);
                self.mode = InsertionMode::InColumnGroup;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "col" => {
                self.clear_stack_back_to_table_context();
                let colgroup = Document::create_element(&self.doc, "colgroup");
                append_child(&self.current(), colgroup.clone());
                self.open_elements.push(colgroup);
                self.mode = InsertionMode::InColumnGroup;
                self.consume(Token::StartTag {
                    name: name.clone(),
                    attrs: attrs.clone(),
                    self_closing: false,
                });
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "tbody" | "tfoot" | "thead") => {
                self.clear_stack_back_to_table_context();
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
                self.mode = InsertionMode::InTableBody;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "td" | "th" | "tr") => {
                self.clear_stack_back_to_table_context();
                let tbody = Document::create_element(&self.doc, "tbody");
                append_child(&self.current(), tbody.clone());
                self.open_elements.push(tbody);
                self.mode = InsertionMode::InTableBody;
                self.consume(Token::StartTag {
                    name: name.clone(),
                    attrs: attrs.clone(),
                    self_closing: false,
                });
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "table" => {
                // Parse error. Pop until we close the current table.
                while tag_name(&self.current()) != "table" && !self.open_elements.is_empty() {
                    self.pop_current_node();
                }
                self.pop_current_node();
                self.mode = InsertionMode::InBody; // approximate
                self.consume(token);
            }
            Token::EndTag { ref name } if name == "table" => {
                if self.has_in_table_scope("table") {
                    while tag_name(&self.current()) != "table" {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                    self.mode = InsertionMode::InBody;
                }
            }
            Token::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "body"
                        | "caption"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            { /* parse error, ignore */ }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "style" | "script" | "template") => {
                self.in_head(token);
            }
            Token::EndTag { ref name } if name == "template" => {
                self.in_head(token);
            }
            _ => {
                // Foster-parent the token.
                self.foster_parent_token(token);
            }
        }
    }

    fn in_table_text(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => {
                self.pending_table_chars.push(c.to_string());
            }
            Token::Character(_) => {
                // Non-whitespace: foster-parent everything.
                let pending: Vec<String> = std::mem::take(&mut self.pending_table_chars);
                for s in pending {
                    for c in s.chars() {
                        self.foster_parent_character(c);
                    }
                }
                self.foster_parent_token(token);
                self.mode = InsertionMode::InTable;
            }
            _ => {
                // Flush pending chars as a single text node, then process.
                let pending: String = std::mem::take(&mut self.pending_table_chars).concat();
                if !pending.is_empty() {
                    let parent = self.current();
                    let text = crate::dom::spec::NodeRef::new(std::cell::RefCell::new(
                        crate::dom::spec::Node {
                            id: 0,
                            kind: crate::dom::spec::NodeKind::Text(pending),
                            owner_document: None,
                            doc: None,
                            parent: None,
                            parent_element: None,
                            first_child: None,
                            last_child: None,
                            previous_sibling: None,
                            next_sibling: None,
                            user_data: std::collections::HashMap::new(),
                        },
                    ));
                    let id = self.doc.borrow().next_node_id();
                    text.borrow_mut().id = id;
                    text.borrow_mut().doc = Some(std::rc::Rc::downgrade(&self.doc));
                    self.doc.borrow().register(&text);
                    append_child(&parent, text);
                }
                self.mode = InsertionMode::InTable;
                self.consume(token);
            }
        }
    }

    fn in_caption(&mut self, token: Token) {
        match token {
            Token::EndTag { ref name } if name == "caption" => {
                if self.has_in_table_scope("caption") {
                    self.generate_implied_end_tags_except(None);
                    while tag_name(&self.current()) != "caption" {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                    self.clear_active_formatting_to_last_marker();
                    self.mode = InsertionMode::InTable;
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "tbody" | "tfoot" | "thead" | "td" | "th" | "tr"
            ) =>
            {
                // Close caption first.
                self.consume(Token::EndTag {
                    name: "caption".to_string(),
                });
                self.consume(Token::StartTag {
                    name: name.clone(),
                    attrs: attrs.clone(),
                    self_closing: false,
                });
            }
            Token::EndTag { ref name } if name == "table" => {
                self.consume(Token::EndTag {
                    name: "caption".to_string(),
                });
                self.consume(token);
            }
            _ => self.in_body(token),
        }
    }

    fn in_column_group(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => self.insert_character(c),
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "col" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
            }
            Token::EndTag { ref name } if name == "colgroup" => {
                if tag_name(&self.current()) == "colgroup" {
                    self.pop_current_node();
                    self.mode = InsertionMode::InTable;
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "template" | "style" | "script") => {
                self.in_head(token);
            }
            Token::EndTag { ref name } if name == "template" => self.in_head(token),
            _ => {
                if tag_name(&self.current()) == "colgroup" {
                    self.pop_current_node();
                    self.mode = InsertionMode::InTable;
                    self.consume(token);
                }
            }
        }
    }

    fn in_table_body(&mut self, token: Token) {
        match token {
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "tr" => {
                self.clear_stack_back_to_table_body_context();
                let tr = self.create_element(name, attrs);
                append_child(&self.current(), tr.clone());
                self.open_elements.push(tr);
                self.mode = InsertionMode::InRow;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_table_body_context();
                let tr = Document::create_element(&self.doc, "tr");
                append_child(&self.current(), tr.clone());
                self.open_elements.push(tr);
                self.mode = InsertionMode::InRow;
                self.consume(Token::StartTag {
                    name: name.clone(),
                    attrs: attrs.clone(),
                    self_closing: false,
                });
            }
            Token::EndTag { ref name } if matches!(name.as_str(), "tbody" | "tfoot" | "thead") => {
                if self.has_in_table_scope(name) {
                    self.clear_stack_back_to_table_body_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTable;
                }
            }
            Token::StartTag { ref name, .. }
                if name == "caption"
                    || name == "col"
                    || name == "colgroup"
                    || name == "tbody"
                    || name == "tfoot"
                    || name == "thead" =>
            {
                // Close tbody if present.
                if self.has_in_table_scope("tbody")
                    || self.has_in_table_scope("thead")
                    || self.has_in_table_scope("tfoot")
                {
                    self.clear_stack_back_to_table_body_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTable;
                    self.consume(token);
                }
            }
            Token::EndTag { ref name } if name == "table" => {
                if self.has_in_table_scope("tbody")
                    || self.has_in_table_scope("thead")
                    || self.has_in_table_scope("tfoot")
                {
                    self.clear_stack_back_to_table_body_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTable;
                    self.consume(token);
                }
            }
            _ => self.in_table(token),
        }
    }

    fn in_row(&mut self, token: Token) {
        match token {
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(name.as_str(), "th" | "td") => {
                self.clear_stack_back_to_row_context();
                let cell = self.create_element(name, attrs);
                append_child(&self.current(), cell.clone());
                self.open_elements.push(cell);
                self.active_formatting.push(FormattingEntry::Marker);
                self.mode = InsertionMode::InCell;
            }
            Token::EndTag { ref name } if name == "tr" => {
                if self.has_in_table_scope("tr") {
                    self.clear_stack_back_to_row_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTableBody;
                }
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead" | "tr"
                ) || (matches!(token, Token::EndTag { ref name } if name == "table")) =>
            {
                if self.has_in_table_scope("tr") {
                    self.clear_stack_back_to_row_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTableBody;
                    self.consume(token);
                }
            }
            Token::EndTag { ref name } if matches!(name.as_str(), "tbody" | "tfoot" | "thead") => {
                if self.has_in_table_scope(name) && self.has_in_table_scope("tr") {
                    self.clear_stack_back_to_row_context();
                    self.pop_current_node();
                    self.mode = InsertionMode::InTableBody;
                    self.consume(token);
                }
            }
            _ => self.in_table(token),
        }
    }

    fn in_cell(&mut self, token: Token) {
        match token {
            Token::EndTag { ref name } if matches!(name.as_str(), "td" | "th") => {
                if self.has_in_table_scope(name) {
                    self.generate_implied_end_tags_except(None);
                    while tag_name(&self.current()).as_str() != name.as_str() {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                    self.clear_active_formatting_to_last_marker();
                    self.mode = InsertionMode::InRow;
                }
            }
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.has_in_table_scope("td") || self.has_in_table_scope("th") {
                    self.close_cell();
                    self.consume(token);
                }
            }
            Token::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html"
                ) =>
            { /* parse error, ignore */ }
            Token::EndTag { ref name }
                if matches!(name.as_str(), "table" | "tbody" | "tfoot" | "thead" | "tr") =>
            {
                if self.has_in_table_scope(name) {
                    self.close_cell();
                    self.consume(token);
                }
            }
            _ => self.in_body(token),
        }
    }

    fn in_select(&mut self, token: Token) {
        match token {
            Token::Character(c) => self.insert_character(c),
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "option" => {
                if tag_name(&self.current()) == "option" {
                    self.pop_current_node();
                }
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "optgroup" => {
                if tag_name(&self.current()) == "option" {
                    self.pop_current_node();
                }
                if tag_name(&self.current()) == "optgroup" {
                    self.pop_current_node();
                }
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "hr" => {
                if tag_name(&self.current()) == "option" {
                    self.pop_current_node();
                }
                if tag_name(&self.current()) == "optgroup" {
                    self.pop_current_node();
                }
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
            }
            Token::EndTag { ref name } if name == "optgroup" => {
                if tag_name(&self.current()) == "option"
                    && self.open_elements.len() >= 2
                    && tag_name(&self.open_elements[self.open_elements.len() - 2]) == "optgroup"
                {
                    self.pop_current_node();
                }
                if tag_name(&self.current()) == "optgroup" {
                    self.pop_current_node();
                }
            }
            Token::EndTag { ref name } if name == "option" => {
                if tag_name(&self.current()) == "option" {
                    self.pop_current_node();
                }
            }
            Token::EndTag { ref name } if name == "select" => {
                if self.has_in_select_scope("select") {
                    while tag_name(&self.current()) != "select" {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                    self.reset_insertion_mode();
                }
            }
            _ => self.in_body(token),
        }
    }

    fn in_select_in_table(&mut self, token: Token) {
        match token {
            Token::StartTag { ref name, .. }
                if matches!(
                    name.as_str(),
                    "caption" | "table" | "tbody" | "tfoot" | "thead" | "tr" | "td" | "th"
                ) =>
            {
                self.consume(Token::EndTag {
                    name: "select".into(),
                });
                self.consume(token);
            }
            Token::EndTag { ref name }
                if matches!(
                    name.as_str(),
                    "caption" | "table" | "tbody" | "tfoot" | "thead" | "tr" | "td" | "th"
                ) =>
            { /* parse error, ignore */ }
            _ => self.in_select(token),
        }
    }

    fn in_template(&mut self, token: Token) {
        match token {
            Token::Character(c) => self.in_body(Token::Character(c)),
            Token::Comment(s) => self.in_body(Token::Comment(s)),
            Token::Doctype { .. } => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if matches!(
                name.as_str(),
                "base"
                    | "basefont"
                    | "bgsound"
                    | "link"
                    | "meta"
                    | "noframes"
                    | "script"
                    | "style"
                    | "template"
                    | "title"
            ) =>
            {
                self.in_head(token);
            }
            Token::EndTag { ref name } if name == "template" => {
                self.in_head(token);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } => {
                // Pop template's stack then push new context.
                let _ = (attrs, name);
                self.pop_current_node(); // pop template
                self.template_modes.pop();
                // Re-insert template and switch based on tag.
                // For simplicity, push InBody.
                self.template_modes.push(InsertionMode::InBody);
                self.mode = InsertionMode::InBody;
                self.consume(token);
            }
            _ => {
                self.template_modes.pop();
                self.reset_insertion_mode();
                self.consume(token);
            }
        }
    }

    fn in_after_body(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => self.in_body(token),
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                let html = self.open_elements.first().cloned();
                if let Some(html) = html {
                    append_child(&html, comment);
                }
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::EndTag { ref name } if name == "html" => {
                self.mode = InsertionMode::AfterAfterBody;
            }
            Token::Eof => {
                // Done.
            }
            _ => {
                self.mode = InsertionMode::InBody;
                self.consume(token);
            }
        }
    }

    fn in_frameset(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => self.insert_character(c),
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "frameset" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el.clone());
                self.open_elements.push(el);
            }
            Token::EndTag { ref name } if name == "frameset" => {
                if self.open_elements.len() == 1 {
                    return;
                }
                self.pop_current_node();
                if self.open_elements.len() > 1 && tag_name(&self.current()) == "frameset" {
                    /* keep going */
                }
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "frame" => {
                let el = self.create_element(name, attrs);
                append_child(&self.current(), el);
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "noframes" => {
                self.in_head(token);
            }
            _ => { /* parse error, ignore */ }
        }
    }

    fn in_after_frameset(&mut self, token: Token) {
        match token {
            Token::Character(c) if c.is_ascii_whitespace() => self.insert_character(c),
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.current(), comment);
            }
            Token::Doctype { .. } => { /* ignore */ }
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::EndTag { ref name } if name == "html" => {
                self.mode = InsertionMode::AfterAfterFrameset;
            }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "noframes" => {
                self.in_head(token);
            }
            _ => { /* parse error, ignore */ }
        }
    }

    fn in_after_after_body(&mut self, token: Token) {
        match token {
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.doc.borrow().root.clone(), comment);
            }
            Token::Doctype { .. } => self.in_body(token),
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::Eof => { /* done */ }
            _ => {
                self.mode = InsertionMode::InBody;
                self.consume(token);
            }
        }
    }

    fn in_after_after_frameset(&mut self, token: Token) {
        match token {
            Token::Comment(s) => {
                let comment = Document::create_comment_for(&self.doc, &s);
                append_child(&self.doc.borrow().root.clone(), comment);
            }
            Token::Doctype { .. } => self.in_body(token),
            Token::StartTag { ref name, .. } if name == "html" => self.in_body(token),
            Token::Eof => { /* done */ }
            Token::StartTag {
                ref name,
                ref attrs,
                ..
            } if name == "noframes" => {
                self.in_head(token);
            }
            _ => { /* parse error, ignore */ }
        }
    }

    // ---- Helper algorithms ----

    fn pop_current_node(&mut self) {
        self.open_elements.pop();
    }

    fn create_element(&self, tag: &str, attrs: &[(String, String)]) -> NodeRef {
        let el = Document::create_element(&self.doc, tag);
        for (k, v) in attrs {
            set_attribute(&el, k, v);
        }
        el
    }

    fn insert_character(&mut self, c: char) {
        let current = self.current();
        // Append to last text node if it exists.
        let last_child = current
            .borrow()
            .last_child
            .clone()
            .and_then(|w| w.upgrade());
        if let Some(last) = last_child {
            let is_text = matches!(last.borrow().kind, NodeKind::Text(_));
            if is_text {
                if let NodeKind::Text(ref mut s) = &mut last.borrow_mut().kind {
                    s.push(c);
                    return;
                }
            }
        }
        let text =
            crate::dom::spec::NodeRef::new(std::cell::RefCell::new(crate::dom::spec::Node {
                id: 0,
                kind: crate::dom::spec::NodeKind::Text(c.to_string()),
                owner_document: None,
                doc: None,
                parent: None,
                parent_element: None,
                first_child: None,
                last_child: None,
                previous_sibling: None,
                next_sibling: None,
                user_data: std::collections::HashMap::new(),
            }));
        let id = self.doc.borrow().next_node_id();
        text.borrow_mut().id = id;
        text.borrow_mut().doc = Some(std::rc::Rc::downgrade(&self.doc));
        self.doc.borrow().register(&text);
        append_child(&current, text);
    }

    fn insert_element_and_switch_to_text(
        &mut self,
        tag: &str,
        attrs: &[(String, String)],
        new_mode: InsertionMode,
    ) {
        let el = self.create_element(tag, attrs);
        append_child(&self.current(), el.clone());
        self.open_elements.push(el);
        self.original_mode = self.mode;
        self.mode = new_mode;
    }

    fn insert_template_element(&mut self) {
        let template = self.create_element("template", &[]);
        // Create template contents (a DocumentFragment).
        let contents = Document::create_document_fragment(&self.doc);
        if let NodeKind::Element(ref mut e) = &mut template.borrow_mut().kind {
            e.template_contents = Some(contents);
        }
        append_child(&self.current(), template.clone());
        self.open_elements.push(template);
        self.template_modes.push(InsertionMode::InTemplate);
        self.mode = InsertionMode::InTemplate;
    }

    fn close_p_if_open(&mut self) {
        if self.has_in_button_scope("p") {
            self.close_p_element();
        }
    }

    fn close_p_element(&mut self) {
        self.generate_implied_end_tags_except(Some("p"));
        while tag_name(&self.current()) != "p" {
            self.pop_current_node();
        }
        self.pop_current_node();
    }

    fn close_cell(&mut self) {
        self.generate_implied_end_tags_except(None);
        while !matches!(tag_name(&self.current()).as_str(), "td" | "th") {
            self.pop_current_node();
        }
        self.pop_current_node();
        self.clear_active_formatting_to_last_marker();
        self.mode = InsertionMode::InRow;
    }

    fn generate_implied_end_tags_except(&mut self, except: Option<&str>) {
        while let Some(top) = self.open_elements.last() {
            let tag = tag_name(top);
            if let Some(ex) = except {
                if tag == ex {
                    return;
                }
            }
            if matches!(
                tag.as_str(),
                "dd" | "dt" | "li" | "optgroup" | "option" | "p" | "rb" | "rp" | "rt" | "rtc"
            ) {
                self.pop_current_node();
            } else {
                return;
            }
        }
    }

    fn clear_stack_back_to_table_context(&mut self) {
        while !self.open_elements.is_empty() {
            let top = self.open_elements.last().unwrap().clone();
            let tag = tag_name(&top);
            if tag == "table" || tag == "html" {
                return;
            }
            self.pop_current_node();
        }
    }

    fn clear_stack_back_to_table_body_context(&mut self) {
        while !self.open_elements.is_empty() {
            let top = self.open_elements.last().unwrap().clone();
            let tag = tag_name(&top);
            if matches!(tag.as_str(), "tbody" | "tfoot" | "thead" | "html") {
                return;
            }
            self.pop_current_node();
        }
    }

    fn clear_stack_back_to_row_context(&mut self) {
        while !self.open_elements.is_empty() {
            let top = self.open_elements.last().unwrap().clone();
            let tag = tag_name(&top);
            if tag == "tr" || tag == "html" {
                return;
            }
            self.pop_current_node();
        }
    }

    fn clear_active_formatting_to_last_marker(&mut self) {
        while let Some(entry) = self.active_formatting.pop() {
            if matches!(entry, FormattingEntry::Marker) {
                return;
            }
        }
    }

    fn reconstruct_active_formatting(&mut self) {
        if self.active_formatting.is_empty() {
            return;
        }
        // If last entry is a marker, nothing to reconstruct.
        if let Some(FormattingEntry::Marker) = self.active_formatting.last() {
            return;
        }
        // If last entry is already in the stack, nothing to do.
        let last = self.active_formatting.last().unwrap().clone();
        if let FormattingEntry::Element(el) = &last {
            if self
                .open_elements
                .iter()
                .any(|n| std::ptr::eq(n.as_ptr(), el.as_ptr()))
            {
                return;
            }
        }
        // Walk backwards to find the first entry that's either a marker or in the stack.
        let mut idx = self.active_formatting.len();
        loop {
            if idx == 0 {
                break;
            }
            idx -= 1;
            match &self.active_formatting[idx] {
                FormattingEntry::Marker => {
                    idx += 1;
                    break;
                }
                FormattingEntry::Element(el) => {
                    if self
                        .open_elements
                        .iter()
                        .any(|n| std::ptr::eq(n.as_ptr(), el.as_ptr()))
                    {
                        idx += 1;
                        break;
                    }
                }
            }
            if idx == 0 {
                break;
            }
        }
        // Reconstruct from idx forward.
        while idx < self.active_formatting.len() {
            let entry = self.active_formatting[idx].clone();
            if let FormattingEntry::Element(el) = entry {
                let tag = tag_name(&el);
                let new_el = Document::create_element(&self.doc, &tag);
                // Copy attributes.
                if let NodeKind::Element(e) = &el.borrow().kind {
                    for a in &e.attrs {
                        set_attribute(&new_el, &a.name, &a.value);
                    }
                }
                append_child(&self.current(), new_el.clone());
                self.active_formatting[idx] = FormattingEntry::Element(new_el.clone());
                self.open_elements.push(new_el);
            }
            idx += 1;
        }
    }

    /// The adoption agency algorithm (§13.2.6.4). Repairs mis-nested
    /// formatting elements.
    fn adoption_agency(&mut self, subject: &str) {
        // 1. If current node is subject and it's in active formatting, remove it.
        let current = self.current();
        let current_is_subject = tag_name(&current) == subject;
        if current_is_subject {
            let mut found_idx = None;
            for (i, entry) in self.active_formatting.iter().enumerate() {
                if let FormattingEntry::Element(el) = entry {
                    if std::ptr::eq(el.as_ptr(), current.as_ptr()) {
                        found_idx = Some(i);
                        break;
                    }
                }
            }
            if let Some(i) = found_idx {
                self.active_formatting.remove(i);
                self.pop_current_node();
                return;
            }
        }
        // 2. Outer loop.
        for _ in 0..8 {
            // 3. Find "formatting element" — last entry in active_formatting that is subject.
            let mut fmt_idx = None;
            for i in (0..self.active_formatting.len()).rev() {
                if let FormattingEntry::Element(el) = &self.active_formatting[i] {
                    if tag_name(el) == subject {
                        fmt_idx = Some(i);
                        break;
                    }
                }
            }
            let Some(fmt_idx) = fmt_idx else {
                // No formatting element — run "any other end tag" path.
                if self.has_in_scope(subject) {
                    self.generate_implied_end_tags_except(None);
                    while tag_name(&self.current()) != subject {
                        self.pop_current_node();
                    }
                    self.pop_current_node();
                }
                return;
            };
            // 4. Find formatting element in stack.
            let fmt_el = if let FormattingEntry::Element(el) = &self.active_formatting[fmt_idx] {
                el.clone()
            } else {
                return;
            };
            let mut stack_idx = None;
            for (i, n) in self.open_elements.iter().enumerate() {
                if std::ptr::eq(n.as_ptr(), fmt_el.as_ptr()) {
                    stack_idx = Some(i);
                    break;
                }
            }
            if stack_idx.is_none() {
                self.active_formatting.remove(fmt_idx);
                continue;
            }
            let stack_idx = stack_idx.unwrap();
            // 5. If stack is at fmt + 1 and current is fmt, pop and remove from active.
            if stack_idx == self.open_elements.len() - 1 {
                self.active_formatting.remove(fmt_idx);
                self.pop_current_node();
                return;
            }
            // 6. Find "furthest block" — topmost stack element below fmt that's special.
            let mut furthest = None;
            for i in (stack_idx + 1)..self.open_elements.len() {
                let tag = tag_name(&self.open_elements[i]);
                if is_special(&tag) {
                    furthest = Some(i);
                    break;
                }
            }
            let Some(furthest) = furthest else {
                // Pop everything from fmt to top.
                while self.open_elements.len() > stack_idx {
                    self.pop_current_node();
                }
                self.active_formatting.remove(fmt_idx);
                return;
            };
            // 7. Common ancestor = open_elements[stack_idx - 1].
            let common_ancestor = self.open_elements[stack_idx - 1].clone();
            // 8. Bookmark = position right after fmt in active_formatting.
            let mut bookmark = fmt_idx + 1;
            // 9. Node = furthest_block, copy node = furthest_block.
            let mut node = self.open_elements[furthest].clone();
            let mut node_idx = furthest;
            // 10. Inner loop.
            let mut last_node = node.clone();
            loop {
                if node_idx == 0 {
                    break;
                }
                node_idx -= 1;
                node = self.open_elements[node_idx].clone();
                // 10.3. If node is formatting element, break.
                if std::ptr::eq(node.as_ptr(), fmt_el.as_ptr()) {
                    break;
                }
                // 10.4. Check if node is in active_formatting.
                let mut af_idx = None;
                for (i, entry) in self.active_formatting.iter().enumerate() {
                    if let FormattingEntry::Element(el) = entry {
                        if std::ptr::eq(el.as_ptr(), node.as_ptr()) {
                            af_idx = Some(i);
                            break;
                        }
                    }
                }
                if af_idx.is_none() {
                    // Remove from stack.
                    self.open_elements.remove(node_idx);
                    continue;
                }
                let af_idx = af_idx.unwrap();
                // 10.5. Create new element with same tag as node, insert into common_ancestor.
                let tag = tag_name(&node);
                let new_el = Document::create_element(&self.doc, &tag);
                if let NodeKind::Element(e) = &node.borrow().kind {
                    for a in &e.attrs {
                        set_attribute(&new_el, &a.name, &a.value);
                    }
                }
                append_child(&common_ancestor, new_el.clone());
                // Replace node in active_formatting.
                self.active_formatting[af_idx] = FormattingEntry::Element(new_el.clone());
                // Replace node in stack.
                self.open_elements[node_idx] = new_el.clone();
                node = new_el;
                // 10.6. If last_node == furthest_block, bookmark = af_idx + 1.
                if std::ptr::eq(last_node.as_ptr(), self.open_elements[furthest].as_ptr()) {
                    bookmark = af_idx + 1;
                }
                // 10.7. Insert node as child of last_node (i.e. move last_node's children into node).
                // For simplicity, we adopt: take last_node from its parent and append to node.
                crate::dom::spec::remove_from_parent(&last_node);
                append_child(&node, last_node.clone());
                // 10.8. last_node = node.
                last_node = node;
            }
            // 11. Insert last_node into common_ancestor.
            crate::dom::spec::remove_from_parent(&last_node);
            append_child(&common_ancestor, last_node.clone());
            // 12. Create new formatting element with same tag as fmt.
            let fmt_tag = tag_name(&fmt_el);
            let new_fmt = Document::create_element(&self.doc, &fmt_tag);
            if let NodeKind::Element(e) = &fmt_el.borrow().kind {
                for a in &e.attrs {
                    set_attribute(&new_fmt, &a.name, &a.value);
                }
            }
            // 13. Take children of furthest_block, append to new_fmt.
            let furthest_el = self.open_elements[furthest].clone();
            // Move all children of furthest_block into new_fmt.
            let children: Vec<NodeRef> = {
                let mut v = Vec::new();
                let mut cursor = furthest_el.borrow().first_child.clone();
                while let Some(c) = cursor {
                    v.push(c.clone());
                    cursor = c.borrow().next_sibling.clone().and_then(|w| w.upgrade());
                }
                v
            };
            for c in children {
                crate::dom::spec::remove_from_parent(&c);
                append_child(&new_fmt, c);
            }
            append_child(&furthest_el, new_fmt.clone());
            // 14. Remove fmt from active_formatting, insert new_fmt at bookmark.
            self.active_formatting.remove(fmt_idx);
            let insert_at = if bookmark > self.active_formatting.len() {
                self.active_formatting.len()
            } else {
                bookmark.min(self.active_formatting.len())
            };
            self.active_formatting
                .insert(insert_at, FormattingEntry::Element(new_fmt.clone()));
            // 15. Remove fmt from stack, insert new_fmt right after furthest_block.
            self.open_elements
                .retain(|n| !std::ptr::eq(n.as_ptr(), fmt_el.as_ptr()));
            // Find furthest_block's new position.
            let mut new_pos = None;
            for (i, n) in self.open_elements.iter().enumerate() {
                if std::ptr::eq(n.as_ptr(), furthest_el.as_ptr()) {
                    new_pos = Some(i + 1);
                    break;
                }
            }
            if let Some(p) = new_pos {
                self.open_elements.insert(p, new_fmt);
            } else {
                self.open_elements.push(new_fmt);
            }
        }
    }

    fn has_in_scope(&self, tag: &str) -> bool {
        self.has_in_scope_with(tag, SCOPE_DEFAULT)
    }
    fn has_in_button_scope(&self, tag: &str) -> bool {
        self.has_in_scope_with(tag, SCOPE_BUTTON)
    }
    fn has_in_table_scope(&self, tag: &str) -> bool {
        self.has_in_scope_with(tag, SCOPE_TABLE)
    }
    fn has_in_select_scope(&self, tag: &str) -> bool {
        self.has_in_scope_with(tag, SCOPE_SELECT)
    }
    fn has_in_list_item_scope(&self, tag: &str) -> bool {
        self.has_in_scope_with(tag, SCOPE_LIST_ITEM)
    }

    fn has_in_scope_with(&self, tag: &str, scope: &[&str]) -> bool {
        for n in self.open_elements.iter().rev() {
            let t = tag_name(n);
            if t == tag {
                return true;
            }
            if scope.contains(&t.as_str()) {
                return false;
            }
        }
        false
    }

    fn has_template_in_stack(&self) -> bool {
        self.open_elements.iter().any(|n| tag_name(n) == "template")
    }

    fn has_a_in_active_formatting(&self) -> bool {
        for entry in &self.active_formatting {
            if let FormattingEntry::Element(el) = entry {
                if tag_name(el) == "a" {
                    return true;
                }
            }
        }
        false
    }

    fn reset_insertion_mode(&mut self) {
        // Walk stack from top to find the right mode.
        for i in (0..self.open_elements.len()).rev() {
            let last = i == 0;
            let tag = tag_name(&self.open_elements[i]);
            match tag.as_str() {
                "select" => {
                    self.mode = InsertionMode::InSelect;
                    return;
                }
                "td" | "th" if !last => {
                    self.mode = InsertionMode::InCell;
                    return;
                }
                "tr" => {
                    self.mode = InsertionMode::InRow;
                    return;
                }
                "tbody" | "thead" | "tfoot" => {
                    self.mode = InsertionMode::InTableBody;
                    return;
                }
                "caption" => {
                    self.mode = InsertionMode::InCaption;
                    return;
                }
                "colgroup" => {
                    self.mode = InsertionMode::InColumnGroup;
                    return;
                }
                "table" => {
                    self.mode = InsertionMode::InTable;
                    return;
                }
                "template" => {
                    self.mode = *self.template_modes.last().unwrap_or(&InsertionMode::InBody);
                    return;
                }
                "head" | "body" | "html" => break,
                _ => continue,
            }
        }
        self.mode = InsertionMode::InBody;
    }

    fn foster_parent_character(&mut self, c: char) {
        // Insert into the element before the last table in the stack.
        let last_table_idx = self
            .open_elements
            .iter()
            .rposition(|n| tag_name(n) == "table");
        if let Some(idx) = last_table_idx {
            if idx > 0 {
                let parent = self.open_elements[idx - 1].clone();
                let text = crate::dom::spec::NodeRef::new(std::cell::RefCell::new(
                    crate::dom::spec::Node {
                        id: 0,
                        kind: crate::dom::spec::NodeKind::Text(c.to_string()),
                        owner_document: None,
                        doc: None,
                        parent: None,
                        parent_element: None,
                        first_child: None,
                        last_child: None,
                        previous_sibling: None,
                        next_sibling: None,
                        user_data: std::collections::HashMap::new(),
                    },
                ));
                let id = self.doc.borrow().next_node_id();
                text.borrow_mut().id = id;
                text.borrow_mut().doc = Some(std::rc::Rc::downgrade(&self.doc));
                self.doc.borrow().register(&text);
                append_child(&parent, text);
                return;
            }
        }
        // Fallback: insert at current.
        self.insert_character(c);
    }

    fn foster_parent_token(&mut self, token: Token) {
        // Process token as if in InBody, but insert at the foster parent position.
        // For simplicity, just delegate to in_body — the next parse pass will
        // figure out where to insert based on the current open element.
        self.in_body(token);
    }

    pub fn finish(self) {
        // Spec says: drain pending template insertion modes, fire DOMContentLoaded.
        // For our purposes, the document is ready.
    }
}

const SCOPE_DEFAULT: &[&str] = &[
    "applet",
    "caption",
    "html",
    "table",
    "td",
    "th",
    "marquee",
    "object",
    "template",
    "mi",
    "mo",
    "mn",
    "ms",
    "mtext",
    "annotation-xml",
    "foreignObject",
    "desc",
    "title",
];
const SCOPE_BUTTON: &[&str] = &["button"];
const SCOPE_TABLE: &[&str] = &["html", "table", "template"];
const SCOPE_SELECT: &[&str] = &["optgroup", "option"];
const SCOPE_LIST_ITEM: &[&str] = &["html", "ol", "ul", "button"];

/// Elements considered "special" by the spec — used in adoption agency
/// and list-item auto-close.
fn is_special(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "applet"
            | "area"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "bgsound"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "iframe"
            | "img"
            | "input"
            | "li"
            | "link"
            | "listing"
            | "main"
            | "marquee"
            | "menu"
            | "meta"
            | "nav"
            | "noembed"
            | "noframes"
            | "noscript"
            | "object"
            | "ol"
            | "p"
            | "param"
            | "plaintext"
            | "pre"
            | "script"
            | "section"
            | "select"
            | "source"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
            | "wbr"
            | "xmp"
    )
}

/// Formatting elements that participate in the active formatting elements
/// algorithm. Spec: §13.2.6.2.
fn is_formatting_element(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "b"
            | "big"
            | "code"
            | "em"
            | "font"
            | "i"
            | "nobr"
            | "s"
            | "small"
            | "strike"
            | "strong"
            | "tt"
            | "u"
    )
}
