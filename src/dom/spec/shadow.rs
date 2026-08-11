//! Shadow DOM — `attachShadow()`, slot distribution, shadow trees.
//!
//! Spec: https://dom.spec.whatwg.org/#shadow-trees
//! + https://html.spec.whatwg.org/multipage/custom-elements.html#shadowtree
//!
//! Shadow DOM allows elements to host a hidden subtree that is separate
//! from the light DOM (the regular children). Slots in the shadow tree
//! pull content from the light DOM by name.
//!
//! Key concepts:
//! * `host` — the element with `attachShadow()` called on it.
//! * `shadowRoot` — the DocumentFragment-like root of the shadow tree.
//! * `slot` — a placeholder in the shadow tree that is "filled" by light DOM.
//! * `assignedNodes` — the light DOM nodes that fill a slot.
//! * `mode: "open" | "closed"` — `closed` shadows are not inspectable from JS.
//!
//! Layout / paint walk the *flattened tree* (light + shadow merged) rather
//! than the raw DOM.

use crate::dom::spec::{append_child, NodeKind, NodeRef, ShadowRootData, ShadowRootMode};
use std::cell::RefCell;
use std::rc::Rc;

/// Errors that can occur when calling attachShadow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachShadowError {
    /// Element does not support shadow DOM (e.g. already has a shadow root).
    AlreadyHasShadow,
    /// Tag does not allow shadow DOM (e.g. `<input>`).
    InvalidHost,
    /// Mode was not "open" or "closed".
    InvalidMode,
}

/// Attach a shadow root to an element.
///
/// Spec: `Element.prototype.attachShadow({ mode: "open" | "closed" })`.
pub fn attach_shadow(host: &NodeRef, mode: &str) -> Result<NodeRef, AttachShadowError> {
    let shadow_mode = match mode {
        "open" => ShadowRootMode::Open,
        "closed" => ShadowRootMode::Closed,
        _ => return Err(AttachShadowError::InvalidMode),
    };

    // Check that host is an Element and doesn't already have a shadow.
    {
        let h = host.borrow();
        if let NodeKind::Element(e) = &h.kind {
            if e.shadow.is_some() {
                return Err(AttachShadowError::AlreadyHasShadow);
            }
            // Some tags don't allow shadow DOM.
            if matches!(
                e.tag.as_str(),
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
                    | "caption"
                    | "col"
                    | "colgroup"
                    | "tbody"
                    | "td"
                    | "tfoot"
                    | "thead"
                    | "tr"
                    | "option"
                    | "optgroup"
                    | "select"
            ) {
                return Err(AttachShadowError::InvalidHost);
            }
        } else {
            return Err(AttachShadowError::InvalidHost);
        }
    }

    // Create the shadow root node. It's a ShadowRoot variant of Node.
    let shadow_root = NodeRef::new(RefCell::new(crate::dom::spec::Node {
        id: 0,
        kind: NodeKind::ShadowRoot(ShadowRootData {
            host: Rc::downgrade(host),
            mode: shadow_mode,
            slots: Vec::new(),
        }),
        owner_document: host.borrow().owner_document.clone(),
        doc: host.borrow().doc.clone(),
        parent: None,
        parent_element: None,
        first_child: None,
        last_child: None,
        previous_sibling: None,
        next_sibling: None,
        user_data: std::collections::HashMap::new(),
    }));
    // Assign an ID via the document.
    if let Some(doc) = host.borrow().doc.as_ref().and_then(|w| w.upgrade()) {
        let id = doc.borrow().next_node_id();
        shadow_root.borrow_mut().id = id;
        doc.borrow().register(&shadow_root);
    }
    // Attach to the host.
    if let NodeKind::Element(ref mut e) = &mut host.borrow_mut().kind {
        e.shadow = Some(shadow_root.clone());
    }
    Ok(shadow_root)
}

/// Get the shadow root of an element. Returns None for closed shadows
/// accessed from outside the shadow tree (matches JS `element.shadowRoot`).
pub fn get_shadow_root(host: &NodeRef) -> Option<NodeRef> {
    if let NodeKind::Element(e) = &host.borrow().kind {
        if let Some(shadow) = &e.shadow {
            // For closed shadows, return None when accessed from outside.
            if let NodeKind::ShadowRoot(sr) = &shadow.borrow().kind {
                if sr.mode == ShadowRootMode::Closed {
                    return None;
                }
            }
            return Some(shadow.clone());
        }
    }
    None
}

/// Manually add a slot to the shadow root's slot list. Called when a
/// `<slot>` element is inserted into the shadow tree.
pub fn register_slot(shadow_root: &NodeRef, slot_name: &str) {
    if let NodeKind::ShadowRoot(ref mut sr) = &mut shadow_root.borrow_mut().kind {
        if !sr.slots.contains(&slot_name.to_string()) {
            sr.slots.push(slot_name.to_string());
        }
    }
}

/// Compute the *flattened tree* — the tree that layout/paint actually walk.
///
/// Algorithm: walk the host's children; for each child, find a matching slot
/// in the host's shadow tree; if found, replace the slot with the child in
/// the flattened tree; if not found, drop the child (or append to default slot).
///
/// This is a simplified version: full slot distribution handles nested
/// slots, slot chianing, fallback content, and re-slotting on mutation.
pub fn flatten_tree(host: &NodeRef) -> Vec<NodeRef> {
    let shadow = if let NodeKind::Element(e) = &host.borrow().kind {
        e.shadow.clone()
    } else {
        None
    };
    if let Some(shadow) = shadow {
        // Walk the shadow tree's children, replacing <slot> with assigned light DOM.
        flatten_with_shadow(&shadow, host)
    } else {
        // No shadow — just return the host's children.
        let mut children = Vec::new();
        let mut cursor = host.borrow().first_child.clone();
        while let Some(child) = cursor {
            children.push(child.clone());
            cursor = child
                .borrow()
                .next_sibling
                .clone()
                .and_then(|w| w.upgrade());
        }
        children
    }
}

fn flatten_with_shadow(shadow_root: &NodeRef, host: &NodeRef) -> Vec<NodeRef> {
    let mut out = Vec::new();
    let mut cursor = shadow_root.borrow().first_child.clone();
    while let Some(child) = cursor {
        if let NodeKind::Element(e) = &child.borrow().kind {
            if e.tag == "slot" {
                // Find the slot's name attribute.
                let slot_name = e
                    .attrs
                    .iter()
                    .find(|a| a.name == "name")
                    .map(|a| a.value.clone())
                    .unwrap_or_default();
                // Find matching children in the host's light DOM.
                let assigned = find_assigned_nodes(host, &slot_name);
                if assigned.is_empty() {
                    // Use the slot's fallback content (its own children).
                    let mut sub_cursor = child.borrow().first_child.clone();
                    while let Some(sub) = sub_cursor {
                        out.push(sub.clone());
                        sub_cursor = sub.borrow().next_sibling.clone().and_then(|w| w.upgrade());
                    }
                } else {
                    for n in assigned {
                        out.push(n);
                    }
                }
                // Move to next sibling.
                cursor = child
                    .borrow()
                    .next_sibling
                    .clone()
                    .and_then(|w| w.upgrade());
                continue;
            }
        }
        out.push(child.clone());
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
    out
}

/// Find light DOM children of `host` that should be assigned to a slot
/// with the given name. If `slot_name` is empty, returns unassigned nodes
/// (those without a `slot` attribute).
fn find_assigned_nodes(host: &NodeRef, slot_name: &str) -> Vec<NodeRef> {
    let mut out = Vec::new();
    let mut cursor = host.borrow().first_child.clone();
    while let Some(child) = cursor {
        let child_slot = if let NodeKind::Element(e) = &child.borrow().kind {
            e.attrs
                .iter()
                .find(|a| a.name == "slot")
                .map(|a| a.value.clone())
        } else {
            None
        };
        match (slot_name, child_slot) {
            ("", None) => out.push(child.clone()),
            (name, Some(s)) if name == s => out.push(child.clone()),
            _ => {}
        }
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
    out
}

/// Find the slot that a given light DOM node is assigned to.
pub fn assigned_slot(node: &NodeRef) -> Option<NodeRef> {
    let parent = node.borrow().parent.as_ref().and_then(|w| w.upgrade())?;
    let shadow = if let NodeKind::Element(e) = &parent.borrow().kind {
        e.shadow.clone()
    } else {
        None
    };
    let shadow = shadow?;
    // Get the node's slot attribute.
    let slot_name = if let NodeKind::Element(e) = &node.borrow().kind {
        e.attrs
            .iter()
            .find(|a| a.name == "slot")
            .map(|a| a.value.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    // Walk shadow tree's slots.
    let mut cursor = shadow.borrow().first_child.clone();
    while let Some(child) = cursor {
        if let NodeKind::Element(e) = &child.borrow().kind {
            if e.tag == "slot" {
                let this_name = e
                    .attrs
                    .iter()
                    .find(|a| a.name == "name")
                    .map(|a| a.value.clone())
                    .unwrap_or_default();
                if this_name == slot_name {
                    return Some(child.clone());
                }
            }
        }
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::{set_attribute, tag_name, Document};

    #[test]
    fn attaches_open_shadow_root() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "div");
        append_child(&root, host.clone());

        let shadow = attach_shadow(&host, "open").unwrap();
        assert!(get_shadow_root(&host).is_some());
        // The shadow root should be the same as what we got.
        assert_eq!(
            shadow.borrow().id,
            get_shadow_root(&host).unwrap().borrow().id
        );
    }

    #[test]
    fn closed_shadow_not_accessible_from_outside() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "div");
        append_child(&root, host.clone());

        let _shadow = attach_shadow(&host, "closed").unwrap();
        // get_shadow_root returns None for closed shadows.
        assert!(get_shadow_root(&host).is_none());
    }

    #[test]
    fn rejects_already_has_shadow() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "div");
        append_child(&root, host.clone());

        attach_shadow(&host, "open").unwrap();
        let result = attach_shadow(&host, "open");
        assert!(matches!(result, Err(AttachShadowError::AlreadyHasShadow)));
    }

    #[test]
    fn rejects_void_elements() {
        let doc = Document::create();
        let img = Document::create_element(&doc, "img");
        let result = attach_shadow(&img, "open");
        assert!(matches!(result, Err(AttachShadowError::InvalidHost)));
    }

    #[test]
    fn slot_distribution_named_slot() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "my-component");
        append_child(&root, host.clone());

        // Light DOM: <div slot="header">Title</div>
        let light_header = Document::create_element(&doc, "div");
        set_attribute(&light_header, "slot", "header");
        let header_text = Document::create_text(&doc, "Title");
        append_child(&light_header, header_text);
        append_child(&host, light_header.clone());

        // Shadow DOM: <slot name="header"></slot>
        let shadow = attach_shadow(&host, "open").unwrap();
        let slot = Document::create_element(&doc, "slot");
        set_attribute(&slot, "name", "header");
        append_child(&shadow, slot);

        // Flatten the tree.
        let flat = flatten_tree(&host);
        // The flat tree should contain the light_header div, not the slot.
        assert_eq!(flat.len(), 1);
        assert_eq!(tag_name(&flat[0]), "div");
    }

    #[test]
    fn slot_distribution_default_slot() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "my-component");
        append_child(&root, host.clone());

        // Light DOM with no slot attribute.
        let text = Document::create_text(&doc, "default content");
        append_child(&host, text);

        // Shadow DOM with unnamed slot.
        let shadow = attach_shadow(&host, "open").unwrap();
        let slot = Document::create_element(&doc, "slot");
        append_child(&shadow, slot);

        let flat = flatten_tree(&host);
        assert_eq!(flat.len(), 1);
        // The text node should be assigned to the unnamed slot.
        assert!(matches!(flat[0].borrow().kind, NodeKind::Text(_)));
    }

    #[test]
    fn slot_fallback_content() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "my-component");
        append_child(&root, host.clone());

        // No light DOM children.
        // Shadow DOM with slot that has fallback content.
        let shadow = attach_shadow(&host, "open").unwrap();
        let slot = Document::create_element(&doc, "slot");
        let fallback = Document::create_text(&doc, "fallback");
        append_child(&slot, fallback);
        append_child(&shadow, slot);

        let flat = flatten_tree(&host);
        assert_eq!(flat.len(), 1);
        // The fallback text should appear.
        assert!(matches!(flat[0].borrow().kind, NodeKind::Text(_)));
    }

    #[test]
    fn assigned_slot_lookup() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let host = Document::create_element(&doc, "my-component");
        append_child(&root, host.clone());

        let light = Document::create_element(&doc, "div");
        set_attribute(&light, "slot", "header");
        append_child(&host, light.clone());

        let shadow = attach_shadow(&host, "open").unwrap();
        let slot = Document::create_element(&doc, "slot");
        set_attribute(&slot, "name", "header");
        append_child(&shadow, slot.clone());

        let found = assigned_slot(&light);
        assert!(found.is_some());
        assert_eq!(found.unwrap().borrow().id, slot.borrow().id);
    }
}
