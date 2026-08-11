//! MutationObserver — observes changes to the DOM tree.
//!
//! Spec: https://dom.spec.whatwg.org/#interface-mutationobserver
//!
//! A MutationObserver watches for tree mutations (childList), attribute
//! changes (attributes), or text content changes (characterData) on a
//! target node and its (optionally) subtree. Records are queued
//! synchronously on mutation and delivered at the next microtask checkpoint
//! by `Document::deliver_mutations()`.

use std::cell::RefCell;
use std::rc::Rc;

use super::NodeRef;

/// A single mutation record. Maps to `MutationRecord` in the DOM spec.
#[derive(Debug, Clone)]
pub struct MutationRecord {
    /// Type of mutation: "childList", "attributes", or "characterData".
    pub kind: MutationKind,
    /// The node on which the observer was registered (or whose subtree
    /// contains the mutation).
    pub target: NodeRef,
    /// For childList: nodes added.
    pub added_nodes: Vec<NodeRef>,
    /// For childList: nodes removed.
    pub removed_nodes: Vec<NodeRef>,
    /// For childList: the previous sibling of the added/removed node.
    pub previous_sibling: Option<NodeRef>,
    /// For childList: the next sibling of the added/removed node.
    pub next_sibling: Option<NodeRef>,
    /// For attributes: the attribute name that changed.
    pub attribute_name: Option<String>,
    /// For attributes: the attribute namespace.
    pub attribute_namespace: Option<String>,
    /// For attributes/characterData: the old value, if observing oldValue.
    pub old_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    ChildList,
    Attributes,
    CharacterData,
}

impl MutationRecord {
    pub fn child_list(target: NodeRef, removed: Option<NodeRef>, added: Option<NodeRef>) -> Self {
        Self {
            kind: MutationKind::ChildList,
            target,
            added_nodes: added.into_iter().collect(),
            removed_nodes: removed.into_iter().collect(),
            previous_sibling: None,
            next_sibling: None,
            attribute_name: None,
            attribute_namespace: None,
            old_value: None,
        }
    }

    pub fn attributes(
        target: NodeRef,
        name: String,
        old_value: Option<String>,
        new_value: Option<String>,
    ) -> Self {
        let _ = new_value;
        Self {
            kind: MutationKind::Attributes,
            target,
            added_nodes: Vec::new(),
            removed_nodes: Vec::new(),
            previous_sibling: None,
            next_sibling: None,
            attribute_name: Some(name),
            attribute_namespace: None,
            old_value,
        }
    }

    pub fn character_data(target: NodeRef, old_value: String) -> Self {
        Self {
            kind: MutationKind::CharacterData,
            target,
            added_nodes: Vec::new(),
            removed_nodes: Vec::new(),
            previous_sibling: None,
            next_sibling: None,
            attribute_name: None,
            attribute_namespace: None,
            old_value: Some(old_value),
        }
    }
}

/// Options for `MutationObserver::observe`.
#[derive(Debug, Clone, Default)]
pub struct MutationObserverInit {
    /// Observe childList changes (insertions/removals).
    pub child_list: bool,
    /// Observe attribute changes.
    pub attributes: bool,
    /// Observe characterData changes on text nodes.
    pub character_data: bool,
    /// Observe the entire subtree rooted at the target.
    pub subtree: bool,
    /// Include old attribute value in records.
    pub attribute_old_value: bool,
    /// Include old character data in records.
    pub character_data_old_value: bool,
    /// Filter attribute observations to this list of names.
    pub attribute_filter: Vec<String>,
}

/// A registered observer.
pub struct MutationObserver {
    /// Registered observations: (target, options).
    pub observations: Vec<(NodeRef, MutationObserverInit)>,
    /// Pending records not yet drained by JS.
    pub records: Vec<MutationRecord>,
    /// Callback to invoke when records are delivered.
    pub callback: Option<Box<dyn Fn(&[MutationRecord])>>,
}

impl std::fmt::Debug for MutationObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationObserver")
            .field("observations", &self.observations.len())
            .field("records", &self.records.len())
            .field("has_callback", &self.callback.is_some())
            .finish()
    }
}

impl MutationObserver {
    pub fn new() -> Self {
        Self {
            observations: Vec::new(),
            records: Vec::new(),
            callback: None,
        }
    }

    pub fn with_callback<F: Fn(&[MutationRecord]) + 'static>(f: F) -> Self {
        let mut o = Self::new();
        o.callback = Some(Box::new(f));
        o
    }

    /// Create a detached observer — used as a placeholder when disconnected.
    pub fn detached() -> Self {
        Self {
            observations: Vec::new(),
            records: Vec::new(),
            callback: None,
        }
    }

    /// Observe a target node with the given options.
    pub fn observe(&mut self, target: NodeRef, options: MutationObserverInit) {
        // If we already observe this target, replace the options.
        if let Some(slot) = self
            .observations
            .iter_mut()
            .find(|(t, _)| t.borrow().id == target.borrow().id)
        {
            slot.1 = options;
        } else {
            self.observations.push((target, options));
        }
    }

    /// Stop observing a target.
    pub fn disconnect_target(&mut self, target: &NodeRef) {
        self.observations
            .retain(|(t, _)| t.borrow().id != target.borrow().id);
    }

    /// Check if a record matches any of our observations.
    pub fn matches(&self, record: &MutationRecord) -> bool {
        for (target, options) in &self.observations {
            // Direct target match.
            if target.borrow().id == record.target.borrow().id {
                let kind_ok = match record.kind {
                    MutationKind::ChildList => options.child_list,
                    MutationKind::Attributes => options.attributes,
                    MutationKind::CharacterData => options.character_data,
                };
                if kind_ok {
                    if record.attribute_name.is_some()
                        && !options.attribute_filter.is_empty()
                        && !options
                            .attribute_filter
                            .contains(record.attribute_name.as_ref().unwrap())
                    {
                        continue;
                    }
                    return true;
                }
            }
            // Subtree match — walk record.target's ancestors to see if any
            // is in our observations with subtree=true.
            if options.subtree {
                let mut cursor = record.target.borrow().parent.clone();
                while let Some(parent_weak) = cursor {
                    if let Some(parent) = parent_weak.upgrade() {
                        if parent.borrow().id == target.borrow().id {
                            let kind_ok = match record.kind {
                                MutationKind::ChildList => options.child_list,
                                MutationKind::Attributes => options.attributes,
                                MutationKind::CharacterData => options.character_data,
                            };
                            if kind_ok {
                                return true;
                            }
                        }
                        cursor = parent.borrow().parent.clone();
                    } else {
                        break;
                    }
                }
            }
        }
        false
    }

    /// Drain pending records.
    pub fn take_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.records)
    }
}

impl Default for MutationObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience alias.
pub type ObserverSlot = Rc<RefCell<MutationObserver>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::{append_child, Document};

    #[test]
    fn observer_receives_child_list() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let parent = Document::create_element(&doc, "div");
        append_child(&root, parent.clone());

        let obs = Rc::new(RefCell::new(MutationObserver::new()));
        obs.borrow_mut().observe(
            parent.clone(),
            MutationObserverInit {
                child_list: true,
                ..Default::default()
            },
        );
        let idx = Document::add_observer(&doc, obs.clone());

        let child = Document::create_element(&doc, "p");
        append_child(&parent, child);

        Document::deliver_mutations(&doc);

        assert!(
            !obs.borrow().records.is_empty(),
            "observer should have records"
        );
        let rec = obs.borrow().records[0].clone();
        assert_eq!(rec.kind, MutationKind::ChildList);
        assert_eq!(rec.added_nodes.len(), 1);

        Document::disconnect_observer(&doc, idx);
    }

    #[test]
    fn observer_receives_attributes() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let parent = Document::create_element(&doc, "div");
        append_child(&root, parent.clone());

        let obs = Rc::new(RefCell::new(MutationObserver::new()));
        obs.borrow_mut().observe(
            parent.clone(),
            MutationObserverInit {
                attributes: true,
                ..Default::default()
            },
        );
        Document::add_observer(&doc, obs.clone());

        crate::dom::spec::set_attribute(&parent, "id", "foo");
        Document::deliver_mutations(&doc);

        assert!(!obs.borrow().records.is_empty());
        let rec = obs.borrow().records[0].clone();
        assert_eq!(rec.kind, MutationKind::Attributes);
        assert_eq!(rec.attribute_name.as_deref(), Some("id"));
    }

    #[test]
    fn subtree_observer_catches_descendant_mutations() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let outer = Document::create_element(&doc, "div");
        append_child(&root, outer.clone());
        let middle = Document::create_element(&doc, "div");
        append_child(&outer, middle.clone());

        let obs = Rc::new(RefCell::new(MutationObserver::new()));
        obs.borrow_mut().observe(
            outer.clone(),
            MutationObserverInit {
                child_list: true,
                subtree: true,
                ..Default::default()
            },
        );
        Document::add_observer(&doc, obs.clone());

        // Add a child to `middle` — should still be observed via subtree.
        let leaf = Document::create_element(&doc, "p");
        append_child(&middle, leaf);

        Document::deliver_mutations(&doc);
        assert!(!obs.borrow().records.is_empty());
    }
}
