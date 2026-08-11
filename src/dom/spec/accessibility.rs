//! Accessibility Tree — parallel to DOM, used by screen readers and ATs.
//!
//! Spec: https://www.w3.org/TR/wai-aria-1.2/
//! + https://www.w3.org/TR/core-aam-1.2/
//!
//! The accessibility tree is a simplified, semantic view of the DOM:
//! * Decorative elements (style, script, etc.) are excluded.
//! * Elements with `role="presentation"` or `aria-hidden="true"` are excluded.
//! * Each visible element is mapped to an accessible node with a role,
//!   name, description, state, and actions.
//!
//! This implementation builds the tree by walking the DOM and applying
//! ARIA role mappings on top of HTML implicit roles.

use crate::dom::spec::{DocumentHandle, NodeKind, NodeRef};

/// A node in the accessibility tree.
#[derive(Debug, Clone)]
pub struct A11yNode {
    /// The role: "button", "link", "heading", "textbox", etc.
    /// Derived from `role` attribute or the element's implicit role.
    pub role: String,
    /// The accessible name — computed per ARIA naming computation.
    /// Sources: aria-label, aria-labelledby, <label>, text content, title.
    pub name: String,
    /// The accessible description (often the `title` attribute).
    pub description: String,
    /// State flags: focused, disabled, checked, expanded, etc.
    pub states: Vec<A11yState>,
    /// Actions the user can perform on this node.
    pub actions: Vec<A11yAction>,
    /// Children in the a11y tree (may differ from DOM children).
    pub children: Vec<A11yNode>,
    /// Back-reference to the DOM node (for hit testing / focus).
    pub dom_node_id: u64,
    /// Level for headings (h1=1, h2=2, etc.) and tree items.
    pub level: Option<u32>,
    /// Value for ranges, text inputs, etc.
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yState {
    Focused,
    Focusable,
    Disabled,
    Hidden,
    Checked,
    Expanded,
    Collapsed,
    Selected,
    Required,
    Readonly,
    Multiline,
    Multiselectable,
    Pressed,
    Visited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yAction {
    Click,
    Focus,
    Toggle,
    Expand,
    Collapse,
    SetValue,
    Select,
}

/// Build the accessibility tree from a document.
pub fn build(doc: &DocumentHandle) -> A11yNode {
    let root = doc.borrow().root.clone();
    build_node(&root)
}

fn build_node(node: &NodeRef) -> A11yNode {
    let mut a11y = A11yNode {
        role: "generic".to_string(),
        name: String::new(),
        description: String::new(),
        states: Vec::new(),
        actions: Vec::new(),
        children: Vec::new(),
        dom_node_id: node.borrow().id,
        level: None,
        value: None,
    };

    let kind = node.borrow().kind.clone();
    match kind {
        NodeKind::Document => {
            a11y.role = "document".to_string();
            walk_children(node, &mut a11y);
        }
        NodeKind::Element(e) => {
            // Skip hidden elements.
            if is_hidden(node) {
                a11y.role = "none".to_string();
                return a11y;
            }
            // Determine role.
            let explicit_role = e
                .attrs
                .iter()
                .find(|a| a.name == "role")
                .map(|a| a.value.clone());
            a11y.role = explicit_role.unwrap_or_else(|| implicit_role(&e.tag));
            if a11y.role == "presentation" || a11y.role == "none" {
                // Children get hoisted to parent.
                walk_children(node, &mut a11y);
                a11y.role = "none".to_string();
                return a11y;
            }
            // Compute name.
            a11y.name = compute_name(node, &a11y.role);
            a11y.description = e
                .attrs
                .iter()
                .find(|a| a.name == "title")
                .map(|a| a.value.clone())
                .unwrap_or_default();
            // Set states.
            if e.focusable {
                a11y.states.push(A11yState::Focusable);
            }
            if let Some(disabled) = e.attrs.iter().find(|a| a.name == "disabled") {
                if disabled.value == "true" || disabled.value.is_empty() {
                    a11y.states.push(A11yState::Disabled);
                }
            }
            // Heading level.
            if e.tag.starts_with('h') && e.tag.len() == 2 {
                if let Ok(level) = e.tag[1..].parse::<u32>() {
                    a11y.level = Some(level);
                }
            }
            // Value for inputs.
            if e.tag == "input" {
                if let Some(v) = e.attrs.iter().find(|a| a.name == "value") {
                    a11y.value = Some(v.value.clone());
                }
            }
            // Actions.
            a11y.actions = actions_for_role(&a11y.role);
            walk_children(node, &mut a11y);
        }
        NodeKind::Text(t) => {
            a11y.role = "text".to_string();
            a11y.name = t;
        }
        _ => {
            a11y.role = "none".to_string();
        }
    }
    a11y
}

fn walk_children(node: &NodeRef, a11y: &mut A11yNode) {
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        let child_a11y = build_node(&child);
        if child_a11y.role != "none" || !child_a11y.name.is_empty() {
            a11y.children.push(child_a11y);
        }
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
}

fn is_hidden(node: &NodeRef) -> bool {
    if let NodeKind::Element(e) = &node.borrow().kind {
        // aria-hidden="true".
        if let Some(hidden) = e.attrs.iter().find(|a| a.name == "aria-hidden") {
            if hidden.value == "true" {
                return true;
            }
        }
        // hidden attribute.
        if e.attrs.iter().any(|a| a.name == "hidden") {
            return true;
        }
        // Inline style with display:none or visibility:hidden.
        // (We don't parse inline styles here — the CSS engine does that.
        // For now, just check the attribute value as a string.)
        if let Some(style) = e.attrs.iter().find(|a| a.name == "style") {
            if style.value.contains("display:none") || style.value.contains("display: none") {
                return true;
            }
        }
    }
    false
}

/// Determine the implicit ARIA role for an HTML tag.
/// Spec: https://www.w3.org/TR/html-aria/
fn implicit_role(tag: &str) -> String {
    match tag {
        "a" => "link".to_string(),
        "button" => "button".to_string(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".to_string(),
        "img" => "img".to_string(),
        "input" => "textbox".to_string(), // simplified; should check type
        "nav" => "navigation".to_string(),
        "main" => "main".to_string(),
        "header" => "banner".to_string(),
        "footer" => "contentinfo".to_string(),
        "form" => "form".to_string(),
        "search" => "search".to_string(),
        "article" => "article".to_string(),
        "section" => "region".to_string(),
        "aside" => "complementary".to_string(),
        "ul" | "ol" => "list".to_string(),
        "li" => "listitem".to_string(),
        "table" => "table".to_string(),
        "tr" => "row".to_string(),
        "td" | "th" => "cell".to_string(),
        "thead" => "rowgroup".to_string(),
        "tbody" => "rowgroup".to_string(),
        "tfoot" => "rowgroup".to_string(),
        "label" => "label".to_string(),
        "p" => "paragraph".to_string(),
        "blockquote" => "blockquote".to_string(),
        "details" => "group".to_string(),
        "summary" => "button".to_string(),
        "dialog" => "dialog".to_string(),
        "select" => "listbox".to_string(),
        "option" => "option".to_string(),
        "textarea" => "textbox".to_string(),
        "output" => "status".to_string(),
        "progress" => "progressbar".to_string(),
        "meter" => "meter".to_string(),
        "figure" => "figure".to_string(),
        "figcaption" => "caption".to_string(),
        "address" => "group".to_string(),
        "code" | "pre" | "samp" | "kbd" | "var" => "code".to_string(),
        "time" => "time".to_string(),
        "mark" => "mark".to_string(),
        "abbr" => "term".to_string(),
        "dfn" => "term".to_string(),
        "caption" => "caption".to_string(),
        "fieldset" => "group".to_string(),
        "legend" => "legend".to_string(),
        "html" => "document".to_string(),
        "head" | "script" | "style" | "meta" | "link" | "title" | "base" => "none".to_string(),
        _ => "generic".to_string(),
    }
}

/// Compute the accessible name per the ARIA naming computation algorithm.
/// Spec: https://www.w3.org/TR/accname-1.2/
///
/// Priority order:
/// 1. aria-labelledby (referenced elements' text content)
/// 2. aria-label attribute
/// 3. Element-specific naming (e.g. <button>'s text content, <input>'s <label>)
/// 4. title attribute
fn compute_name(node: &NodeRef, role: &str) -> String {
    if let NodeKind::Element(e) = &node.borrow().kind {
        // 1. aria-labelledby.
        if let Some(labelledby) = e.attrs.iter().find(|a| a.name == "aria-labelledby") {
            // We don't have access to the document here to look up IDs.
            // In a full impl we'd walk the IDs and concatenate their text.
            // For now, return empty — the caller can resolve this.
            let _ = labelledby;
        }
        // 2. aria-label.
        if let Some(label) = e.attrs.iter().find(|a| a.name == "aria-label") {
            return label.value.clone();
        }
        // 3. Element-specific naming.
        match e.tag.as_str() {
            "button" | "a" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary" | "caption"
            | "figcaption" | "legend" => {
                return text_content(node);
            }
            "input" => {
                // Use value, placeholder, or title.
                if let Some(v) = e.attrs.iter().find(|a| a.name == "value") {
                    return v.value.clone();
                }
                if let Some(p) = e.attrs.iter().find(|a| a.name == "placeholder") {
                    return p.value.clone();
                }
            }
            "img" => {
                if let Some(alt) = e.attrs.iter().find(|a| a.name == "alt") {
                    return alt.value.clone();
                }
            }
            _ => {}
        }
        // 4. title attribute.
        if let Some(title) = e.attrs.iter().find(|a| a.name == "title") {
            return title.value.clone();
        }
    }
    if role == "text" {
        if let NodeKind::Text(t) = &node.borrow().kind {
            return t.clone();
        }
    }
    String::new()
}

/// Get the text content of a node (concatenation of all descendant text).
fn text_content(node: &NodeRef) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out.trim().to_string()
}

fn collect_text(node: &NodeRef, out: &mut String) {
    let kind = node.borrow().kind.clone();
    if let NodeKind::Text(t) = kind {
        out.push_str(&t);
        return;
    }
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        collect_text(&child, out);
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
}

/// Determine which actions a role supports.
fn actions_for_role(role: &str) -> Vec<A11yAction> {
    match role {
        "button" | "link" | "summary" => vec![A11yAction::Click, A11yAction::Focus],
        "textbox" | "searchbox" | "spinbutton" => vec![A11yAction::Focus, A11yAction::SetValue],
        "checkbox" | "radio" | "switch" => {
            vec![A11yAction::Click, A11yAction::Focus, A11yAction::Toggle]
        }
        "option" => vec![A11yAction::Click, A11yAction::Focus, A11yAction::Select],
        "treeitem" => vec![
            A11yAction::Click,
            A11yAction::Focus,
            A11yAction::Expand,
            A11yAction::Collapse,
        ],
        "tab" => vec![A11yAction::Click, A11yAction::Focus, A11yAction::Select],
        "menuitem" => vec![A11yAction::Click, A11yAction::Focus],
        "slider" | "scrollbar" => vec![A11yAction::Focus, A11yAction::SetValue],
        _ => vec![A11yAction::Focus],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::{append_child, set_attribute, Document};

    #[test]
    fn builds_tree_for_button() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let button = Document::create_element(&doc, "button");
        let text = Document::create_text(&doc, "Submit");
        append_child(&button, text);
        append_child(&root, button);

        let a11y = build(&doc);
        // Walk to find the button node.
        let btn = find_by_role(&a11y, "button");
        assert!(btn.is_some(), "button should be in a11y tree");
        assert_eq!(btn.unwrap().name, "Submit");
    }

    #[test]
    fn skips_hidden_elements() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let div = Document::create_element(&doc, "div");
        set_attribute(&div, "aria-hidden", "true");
        let text = Document::create_text(&doc, "hidden text");
        append_child(&div, text);
        append_child(&root, div);

        let a11y = build(&doc);
        assert!(
            find_by_role(&a11y, "generic").is_none()
                || find_by_name(&a11y, "hidden text").is_none()
        );
    }

    #[test]
    fn uses_aria_label_when_present() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let button = Document::create_element(&doc, "button");
        set_attribute(&button, "aria-label", "Close dialog");
        append_child(&root, button);

        let a11y = build(&doc);
        let btn = find_by_role(&a11y, "button").unwrap();
        assert_eq!(btn.name, "Close dialog");
    }

    #[test]
    fn assigns_heading_level() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let h2 = Document::create_element(&doc, "h2");
        let text = Document::create_text(&doc, "Section");
        append_child(&h2, text);
        append_child(&root, h2);

        let a11y = build(&doc);
        let heading = find_by_role(&a11y, "heading").unwrap();
        assert_eq!(heading.level, Some(2));
    }

    #[test]
    fn honors_explicit_role_attribute() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let div = Document::create_element(&doc, "div");
        set_attribute(&div, "role", "button");
        let text = Document::create_text(&doc, "Click me");
        append_child(&div, text);
        append_child(&root, div);

        let a11y = build(&doc);
        let btn = find_by_role(&a11y, "button");
        assert!(btn.is_some(), "div with role=button should map to button");
    }

    fn find_by_role<'a>(node: &'a A11yNode, role: &str) -> Option<&'a A11yNode> {
        if node.role == role {
            return Some(node);
        }
        for child in &node.children {
            if let Some(n) = find_by_role(child, role) {
                return Some(n);
            }
        }
        None
    }

    fn find_by_name<'a>(node: &'a A11yNode, name: &str) -> Option<&'a A11yNode> {
        if node.name == name {
            return Some(node);
        }
        for child in &node.children {
            if let Some(n) = find_by_name(child, name) {
                return Some(n);
            }
        }
        None
    }
}
