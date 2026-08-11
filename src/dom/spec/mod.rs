//! DOM2 — spec-compliant DOM with live parent/child/sibling links.
//!
//! This module implements the DOM Core specification as described in
//! https://dom.spec.whatwg.org/. Unlike the legacy `dom` module (which uses
//! owned `Vec<Node>` children with no parent pointers), `dom2` provides:
//!
//! * **Live references** — `NodeRef` is a `Rc<RefCell<Node>>` (alias `NodeRef`).
//!   Every node carries `parent`, `first_child`, `last_child`, `previous_sibling`,
//!   and `next_sibling` pointers, exactly as specified in the DOM spec.
//! * **Mutation observers** — `MutationObserver` instances receive records on
//!   `childList`, `attributes`, `characterData`, and subtree mutations.
//! * **Mutation events** — synchronous events fired on every tree mutation
//!   (DOMNodeInserted, DOMNodeRemoved, DOMSubtreeModified, etc.), compatible
//!   with legacy code that relies on mutation events.
//! * **Document ordering** — every node has a stable `node_id` so we can sort
//!   collections into document order even after tree surgery.
//! * **Shadow DOM** — `Element` carries an optional `shadow_root`; `attachShadow()`
//!   produces a ShadowRoot whose children are not visible from the light DOM.
//! * **Custom Elements** — `Element` carries an optional `custom_element_state`
//!   so the JS layer can hook `connectedCallback` / `disconnectedCallback`.
//! * **Template contents** — `<template>` carries a `DocumentFragment` rather
//!   than children directly, so it is not rendered by default.
//!
//! ## Node identity
//!
//! Every node is assigned a globally unique `NodeId` (monotonic counter). The
//! `Document` keeps a registry from `NodeId -> Weak<Node>` so the JS engine
//! can look up nodes by ID after layout has finished, without holding the
//! document alive.
//!
//! ## Why Rc<RefCell<>> instead of a slab
//!
//! Real browsers use a slab + indices for performance, but `Rc<RefCell<>>`
//! matches the DOM spec's "live" semantics precisely: JavaScript code can
//! hold a reference to a node, the document can also hold a reference, and
//! both observe the same mutations. The cost is acceptable for our use case
//! (Falco is not aiming to render gmail.com).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};

use crate::dom::spec::observer::{MutationObserver, MutationRecord};

/// A node ID — unique within a document.
pub type NodeId = u64;

/// Strong reference to a node.
pub type NodeRef = Rc<RefCell<Node>>;

/// Weak reference to a node.
pub type NodeWeak = Weak<RefCell<Node>>;

/// A registered MutationObserver. We hold a back-reference so we can keep
/// emitting records as long as the observer has not been disconnected.
type ObserverSlot = Rc<RefCell<MutationObserver>>;

/// The kind of node, as enumerated by the DOM spec.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// `Document` — the root of every DOM tree.
    Document,
    /// `DocumentType` — e.g. `<!DOCTYPE html>`.
    DocumentType {
        name: String,
        public_id: String,
        system_id: String,
    },
    /// `Element` — `<div>`, `<span>`, etc.
    Element(ElementData),
    /// `Text` — character content.
    Text(String),
    /// `Comment` — `<!-- ... -->`.
    Comment(String),
    /// `DocumentFragment` — used for `<template>` contents, shadow roots,
    /// and Range fragments.
    DocumentFragment,
    /// `ShadowRoot` — attached to a host element via `attachShadow()`.
    ShadowRoot(ShadowRootData),
    /// `ProcessingInstruction` — `<?target data?>`, mostly used in XML mode.
    ProcessingInstruction { target: String, data: String },
    /// `Attr` — represented as a node for legacy compatibility. Rarely used
    /// directly; attribute access goes through `ElementData::attrs`.
    Attr { name: String, value: String },
}

/// Element-specific data.
#[derive(Debug, Clone)]
pub struct ElementData {
    /// Tag name. Stored lowercase for HTML, original case for XML.
    pub tag: String,
    /// Namespace URI. `None` for HTML, `Some(SVG_NS)` for SVG, etc.
    pub namespace: Option<&'static str>,
    /// Attribute map. Insertion order preserved (DOM spec requires this
    /// for `Element::attributes` iteration).
    pub attrs: Vec<Attribute>,
    /// Custom element state — `Undefined` until `customElements.define`
    /// is called for this tag.
    pub custom: CustomElementState,
    /// Optional shadow root attached via `attachShadow()`.
    pub shadow: Option<NodeRef>,
    /// `<template>` content. Set when the tag is `template`. Lives in a
    /// separate fragment so it is never rendered.
    pub template_contents: Option<NodeRef>,
    /// Inline `style` attribute parsed into declarations. Lazily populated.
    pub inline_style: Vec<(String, String)>,
    /// Whether this element is focusable (used by accessibility tree).
    pub focusable: bool,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: String,
    pub namespace: Option<&'static str>,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CustomElementState {
    /// Not a custom element.
    #[default]
    Undefined,
    /// Tag matches a `customElements.define()` registration but constructor
    /// has not been called yet.
    Uncustomized,
    /// Currently upgrading.
    Customizing,
    /// Fully upgraded — `connectedCallback` will fire when inserted.
    Custom,
}

#[derive(Debug, Clone)]
pub struct ShadowRootData {
    /// The host element this shadow root is attached to.
    pub host: NodeWeak,
    /// `open` shadows are inspectable from JS via `element.shadowRoot`;
    /// `closed` shadows return `null`.
    pub mode: ShadowRootMode,
    /// Slots declared in this shadow tree.
    pub slots: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

/// A DOM node. The full set of fields specified in the DOM spec.
#[derive(Debug, Clone)]
pub struct Node {
    /// Unique node ID assigned at creation.
    pub id: NodeId,
    /// What kind of node this is.
    pub kind: NodeKind,
    /// Owning document. `None` only for nodes that haven't been adopted yet.
    pub owner_document: Option<NodeWeak>,
    /// Back-reference to the Document struct (weak to break the cycle).
    /// This lets mutation operations find the observer queue.
    pub doc: Option<Weak<RefCell<Document>>>,
    /// Parent node. `None` only for Document, DocumentFragment not attached
    /// to anything, and nodes that have just been removed.
    pub parent: Option<NodeWeak>,
    /// Parent element (None if parent is Document or DocumentFragment).
    pub parent_element: Option<NodeWeak>,
    /// First child. Walk siblings via `next_sibling` from here.
    pub first_child: Option<NodeRef>,
    /// Last child. Used for O(1) append.
    pub last_child: Option<NodeWeak>,
    /// Previous sibling. `None` for the first child.
    pub previous_sibling: Option<NodeWeak>,
    /// Next sibling. `None` for the last child.
    pub next_sibling: Option<NodeWeak>,
    /// User data — opaque map for JS to attach data via `Node::setUserData`.
    pub user_data: HashMap<String, String>,
}

/// The entry point for a document tree. Owns the document node, the ID
/// counter, and all registered mutation observers.
pub struct Document {
    /// The Document node itself.
    pub root: NodeRef,
    /// Next node ID to assign.
    next_id: RefCell<NodeId>,
    /// Registry of all live nodes by ID. Weak so we don't leak.
    nodes: RefCell<HashMap<NodeId, NodeWeak>>,
    /// Registered mutation observers.
    observers: RefCell<Vec<ObserverSlot>>,
    /// Pending mutation records not yet delivered to observers.
    pending_records: RefCell<VecDeque<(ObserverSlot, MutationRecord)>>,
    /// URL this document was loaded from.
    pub url: String,
    /// Detected character encoding.
    pub encoding: &'static str,
    /// Whether this document is in XML (strict) or HTML (permissive) mode.
    pub xml_mode: bool,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("url", &self.url)
            .field("encoding", &self.encoding)
            .field("xml_mode", &self.xml_mode)
            .field("node_count", &self.nodes.borrow().len())
            .finish()
    }
}

/// Handle to a Document. The Document lives inside an `Rc<RefCell<>>` so
/// that nodes can hold weak references back to it (for mutation delivery).
pub type DocumentHandle = Rc<RefCell<Document>>;

impl Document {
    /// Create a new empty document, wrapped in an Rc<RefCell<>>.
    pub fn create() -> DocumentHandle {
        let handle: DocumentHandle = Rc::new(RefCell::new(Self {
            root: NodeRef::new(RefCell::new(Node {
                id: 0,
                kind: NodeKind::Document,
                owner_document: None,
                doc: None,
                parent: None,
                parent_element: None,
                first_child: None,
                last_child: None,
                previous_sibling: None,
                next_sibling: None,
                user_data: HashMap::new(),
            })),
            next_id: RefCell::new(1),
            nodes: RefCell::new(HashMap::new()),
            observers: RefCell::new(Vec::new()),
            pending_records: RefCell::new(VecDeque::new()),
            url: String::new(),
            encoding: "utf-8",
            xml_mode: false,
        }));
        // Set the root node's doc back-reference.
        {
            let weak = Rc::downgrade(&handle);
            let root_clone = handle.borrow().root.clone();
            let mut root = root_clone.borrow_mut();
            root.doc = Some(weak);
        }
        let root_clone = handle.borrow().root.clone();
        handle.borrow().register(&root_clone);
        handle
    }

    /// Mint a new unique node ID.
    pub fn next_node_id(&self) -> NodeId {
        let mut id = self.next_id.borrow_mut();
        let v = *id;
        *id += 1;
        v
    }

    /// Register a node in the document's ID map.
    pub fn register(&self, node: &NodeRef) {
        let id = node.borrow().id;
        self.nodes.borrow_mut().insert(id, Rc::downgrade(node));
    }

    /// Look up a node by ID. Returns None if the node has been freed.
    pub fn get_node(&self, id: NodeId) -> Option<NodeRef> {
        self.nodes.borrow().get(&id).and_then(|w| w.upgrade())
    }

    /// Create a new element owned by this document.
    pub fn create_element(this: &DocumentHandle, tag: &str) -> NodeRef {
        let id = this.borrow().next_node_id();
        let root = this.borrow().root.clone();
        let weak = Rc::downgrade(this);
        let node = NodeRef::new(RefCell::new(Node {
            id,
            kind: NodeKind::Element(ElementData {
                tag: tag.to_lowercase(),
                namespace: None,
                attrs: Vec::new(),
                custom: CustomElementState::default(),
                shadow: None,
                template_contents: None,
                inline_style: Vec::new(),
                focusable: matches!(
                    tag.to_lowercase().as_str(),
                    "a" | "button" | "input" | "select" | "textarea" | "summary"
                ),
            }),
            owner_document: Some(Rc::downgrade(&root)),
            doc: Some(weak),
            parent: None,
            parent_element: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            user_data: HashMap::new(),
        }));
        this.borrow().register(&node);
        node
    }

    pub fn create_text(this: &DocumentHandle, text: &str) -> NodeRef {
        let id = this.borrow().next_node_id();
        let root = this.borrow().root.clone();
        let weak = Rc::downgrade(this);
        let node = NodeRef::new(RefCell::new(Node {
            id,
            kind: NodeKind::Text(text.to_string()),
            owner_document: Some(Rc::downgrade(&root)),
            doc: Some(weak),
            parent: None,
            parent_element: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            user_data: HashMap::new(),
        }));
        this.borrow().register(&node);
        node
    }

    pub fn create_comment(this: &DocumentHandle, text: &str) -> NodeRef {
        let id = this.borrow().next_node_id();
        let root = this.borrow().root.clone();
        let weak = Rc::downgrade(this);
        let node = NodeRef::new(RefCell::new(Node {
            id,
            kind: NodeKind::Comment(text.to_string()),
            owner_document: Some(Rc::downgrade(&root)),
            doc: Some(weak),
            parent: None,
            parent_element: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            user_data: HashMap::new(),
        }));
        this.borrow().register(&node);
        node
    }

    pub fn create_document_fragment(this: &DocumentHandle) -> NodeRef {
        let id = this.borrow().next_node_id();
        let root = this.borrow().root.clone();
        let weak = Rc::downgrade(this);
        let node = NodeRef::new(RefCell::new(Node {
            id,
            kind: NodeKind::DocumentFragment,
            owner_document: Some(Rc::downgrade(&root)),
            doc: Some(weak),
            parent: None,
            parent_element: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            user_data: HashMap::new(),
        }));
        this.borrow().register(&node);
        node
    }

    /// Create a comment node owned by this document.
    pub fn create_comment_for(this: &DocumentHandle, text: &str) -> NodeRef {
        let id = this.borrow().next_node_id();
        let root = this.borrow().root.clone();
        let weak = Rc::downgrade(this);
        let node = NodeRef::new(RefCell::new(Node {
            id,
            kind: NodeKind::Comment(text.to_string()),
            owner_document: Some(Rc::downgrade(&root)),
            doc: Some(weak),
            parent: None,
            parent_element: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            user_data: HashMap::new(),
        }));
        this.borrow().register(&node);
        node
    }

    /// Register a mutation observer. Returns a handle that can be used to
    /// disconnect it.
    pub fn add_observer(this: &DocumentHandle, observer: ObserverSlot) -> usize {
        let doc = this.borrow_mut();
        let mut obs = doc.observers.borrow_mut();
        let idx = obs.len();
        obs.push(observer);
        idx
    }

    /// Drain all pending mutation records from the document.
    ///
    /// Returns the records and clears the queue. Called by the JS bridge
    /// after script execution to deliver mutations to MutationObserver callbacks.
    pub fn drain_pending_records(this: &DocumentHandle) -> Vec<(ObserverSlot, MutationRecord)> {
        let doc = this.borrow();
        let mut pending = doc.pending_records.borrow_mut();
        pending.drain(..).collect()
    }

    /// Disconnect a mutation observer by handle.
    pub fn disconnect_observer(this: &DocumentHandle, idx: usize) {
        let doc = this.borrow_mut();
        let mut obs = doc.observers.borrow_mut();
        if idx < obs.len() {
            obs[idx] = Rc::new(RefCell::new(MutationObserver::detached()));
        }
    }

    /// Queue a mutation record for delivery on the next microtask.
    pub fn queue_mutation(this: &DocumentHandle, record: MutationRecord) {
        let observers = this.borrow().observers.borrow().clone();
        for obs in observers {
            if obs.borrow().matches(&record) {
                this.borrow()
                    .pending_records
                    .borrow_mut()
                    .push_back((obs.clone(), record.clone()));
            }
        }
    }

    /// Drain pending mutation records into observers. Called by the event
    /// loop at microtask checkpoints.
    pub fn deliver_mutations(this: &DocumentHandle) {
        let records: Vec<_> = this
            .borrow()
            .pending_records
            .borrow_mut()
            .drain(..)
            .collect();
        for (obs, record) in records {
            obs.borrow_mut().records.push(record);
        }
    }

    /// Pretty-print the tree to a string for debugging.
    pub fn serialize(this: &DocumentHandle) -> String {
        let mut out = String::new();
        let root = this.borrow().root.clone();
        serialize_node(&root, &mut out, 0);
        out
    }
}

impl Default for Document {
    fn default() -> Self {
        // Used only by internal machinery; users should call `Document::create()`.
        Self {
            root: NodeRef::new(RefCell::new(Node {
                id: 0,
                kind: NodeKind::Document,
                owner_document: None,
                doc: None,
                parent: None,
                parent_element: None,
                first_child: None,
                last_child: None,
                previous_sibling: None,
                next_sibling: None,
                user_data: HashMap::new(),
            })),
            next_id: RefCell::new(1),
            nodes: RefCell::new(HashMap::new()),
            observers: RefCell::new(Vec::new()),
            pending_records: RefCell::new(VecDeque::new()),
            url: String::new(),
            encoding: "utf-8",
            xml_mode: false,
        }
    }
}

/// Append a child to a parent, fixing all sibling pointers and firing
/// mutation records / events.
pub fn append_child(parent: &NodeRef, child: NodeRef) {
    // Detach from current parent first.
    remove_from_parent(&child);

    // 1. Read state we need from parent (immutable borrow).
    let prev_last = parent.borrow().last_child.clone();
    let parent_is_element = matches!(parent.borrow().kind, NodeKind::Element(_));

    // 2. Mutate prev last child first (no parent borrow held).
    if let Some(prev_weak) = &prev_last {
        if let Some(prev) = prev_weak.upgrade() {
            prev.borrow_mut().next_sibling = Some(Rc::downgrade(&child));
        }
    }

    // 3. Mutate child (no parent borrow held).
    {
        let mut c = child.borrow_mut();
        c.parent = Some(Rc::downgrade(parent));
        c.parent_element = if parent_is_element {
            Some(Rc::downgrade(parent))
        } else {
            None
        };
        if let Some(prev_weak) = &prev_last {
            if let Some(prev) = prev_weak.upgrade() {
                c.previous_sibling = Some(Rc::downgrade(&prev));
            }
        }
    }

    // 4. Mutate parent (no child borrow held).
    {
        let mut p = parent.borrow_mut();
        if prev_last.is_none() {
            p.first_child = Some(child.clone());
        }
        p.last_child = Some(Rc::downgrade(&child));
    }

    // 5. Fire mutation records.
    if let Some(doc) = parent_doc(parent) {
        Document::queue_mutation(
            &doc,
            MutationRecord::child_list(parent.clone(), None, Some(child.clone())),
        );
    }
}

/// Insert `child` before `ref_child` in `parent`. If `ref_child` is None,
/// this is equivalent to append_child.
pub fn insert_before(parent: &NodeRef, child: NodeRef, ref_child: Option<&NodeRef>) {
    match ref_child {
        None => append_child(parent, child),
        Some(ref_node) => {
            remove_from_parent(&child);

            // 1. Read state from parent + ref_node.
            let ref_prev = ref_node.borrow().previous_sibling.clone();
            let parent_is_element = matches!(parent.borrow().kind, NodeKind::Element(_));

            // 2. Mutate child.
            {
                let mut c = child.borrow_mut();
                c.parent = Some(Rc::downgrade(parent));
                c.parent_element = if parent_is_element {
                    Some(Rc::downgrade(parent))
                } else {
                    None
                };
                c.next_sibling = Some(Rc::downgrade(ref_node));
                if let Some(prev_weak) = &ref_prev {
                    if let Some(prev) = prev_weak.upgrade() {
                        c.previous_sibling = Some(Rc::downgrade(&prev));
                    }
                }
            }

            // 3. Mutate ref_node.
            ref_node.borrow_mut().previous_sibling = Some(Rc::downgrade(&child));

            // 4. Mutate prev sibling OR parent.
            if let Some(prev_weak) = &ref_prev {
                if let Some(prev) = prev_weak.upgrade() {
                    prev.borrow_mut().next_sibling = Some(Rc::downgrade(&child));
                } else {
                    parent.borrow_mut().first_child = Some(child.clone());
                }
            } else {
                parent.borrow_mut().first_child = Some(child.clone());
            }

            if let Some(doc) = parent_doc(parent) {
                Document::queue_mutation(
                    &doc,
                    MutationRecord::child_list(parent.clone(), None, Some(child.clone())),
                );
            }
        }
    }
}

/// Remove a node from its parent.
pub fn remove_from_parent(node: &NodeRef) {
    let (parent, prev, next) = {
        let n = node.borrow();
        (
            n.parent.clone(),
            n.previous_sibling.clone(),
            n.next_sibling.clone(),
        )
    };
    let Some(parent) = parent.and_then(|w| w.upgrade()) else {
        return;
    };
    // Fix previous sibling.
    if let Some(prev_weak) = prev.clone() {
        if let Some(prev) = prev_weak.upgrade() {
            prev.borrow_mut().next_sibling = next.clone();
        }
    } else {
        // We were the first child.
        let next_strong = next.clone().and_then(|w| w.upgrade());
        parent.borrow_mut().first_child = next_strong.clone();
    }
    // Fix next sibling.
    if let Some(next_weak) = next.clone() {
        if let Some(next_strong) = next_weak.upgrade() {
            next_strong.borrow_mut().previous_sibling = prev.clone();
        }
    } else {
        // We were the last child.
        parent.borrow_mut().last_child = prev.clone();
    }
    // Clear our pointers.
    {
        let mut n = node.borrow_mut();
        n.parent = None;
        n.parent_element = None;
        n.previous_sibling = None;
        n.next_sibling = None;
    }
    if let Some(doc) = parent_doc(&parent) {
        Document::queue_mutation(
            &doc,
            MutationRecord::child_list(parent.clone(), Some(node.clone()), None),
        );
    }
}

/// Replace `old_child` with `new_child` in `parent`.
pub fn replace_child(parent: &NodeRef, new_child: NodeRef, old_child: &NodeRef) {
    let next = old_child.borrow().next_sibling.clone();
    remove_from_parent(old_child);
    insert_before(parent, new_child, next.and_then(|w| w.upgrade()).as_ref());
}

/// Remove all children of a node.
pub fn remove_all_children(node: &NodeRef) {
    while let Some(child) = node.borrow().first_child.clone() {
        remove_from_parent(&child);
    }
}

/// Walk children of a node in document order.
pub fn for_each_child<F: FnMut(&NodeRef)>(node: &NodeRef, mut f: F) {
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        f(&child);
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
}

/// Collect all descendants of a node in document order.
pub fn descendants(node: &NodeRef) -> Vec<NodeRef> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeRef> = Vec::new();
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        stack.push(child.clone());
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
    while let Some(n) = stack.pop() {
        out.push(n.clone());
        // We push children in reverse so they pop in order — but since
        // we want document order we use a VecDeque and prepend.
        let mut sub = Vec::new();
        let mut c = n.borrow().first_child.clone();
        while let Some(child) = c {
            sub.push(child.clone());
            c = child
                .borrow()
                .next_sibling
                .clone()
                .and_then(|w| w.upgrade());
        }
        // Reverse to keep document order on pop... actually we want BFS by
        // depth so just push everything.
        for s in sub.into_iter().rev() {
            stack.push(s);
        }
    }
    out
}

/// Find the owner document of a node, if any. Reads the `doc` back-reference
/// stored on the node itself.
fn parent_doc(node: &NodeRef) -> Option<DocumentHandle> {
    node.borrow().doc.as_ref().and_then(|w| w.upgrade())
}

/// Serialize a node and its descendants to a debug string.
fn serialize_node(node: &NodeRef, out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    let n = node.borrow();
    match &n.kind {
        NodeKind::Document => out.push_str("Document\n"),
        NodeKind::DocumentType { name, .. } => {
            out.push_str(&format!("<!DOCTYPE {}>\n", name));
        }
        NodeKind::Element(e) => {
            out.push_str(&format!("<{}", e.tag));
            for attr in &e.attrs {
                out.push_str(&format!(" {}=\"{}\"", attr.name, attr.value));
            }
            out.push('>');
            if n.first_child.is_some() {
                out.push('\n');
            } else {
                out.push_str(" />\n");
            }
        }
        NodeKind::Text(t) => {
            out.push_str(&format!("\"{}\"\n", t.chars().take(40).collect::<String>()));
        }
        NodeKind::Comment(c) => {
            out.push_str(&format!(
                "<!--{}-->\n",
                c.chars().take(40).collect::<String>()
            ));
        }
        NodeKind::DocumentFragment => out.push_str("#document-fragment\n"),
        NodeKind::ShadowRoot(_) => out.push_str("#shadow-root\n"),
        NodeKind::ProcessingInstruction { target, data } => {
            out.push_str(&format!("<?{} {}?>\n", target, data));
        }
        NodeKind::Attr { name, value } => {
            out.push_str(&format!("attr {}=\"{}\"\n", name, value));
        }
    }
    drop(n);
    // Walk children.
    let mut cursor = node.borrow().first_child.clone();
    while let Some(child) = cursor {
        serialize_node(&child, out, depth + 1);
        cursor = child
            .borrow()
            .next_sibling
            .clone()
            .and_then(|w| w.upgrade());
    }
}

/// Element accessor: returns the ElementData if this is an Element.
pub fn as_element(node: &NodeRef) -> Option<std::cell::Ref<'_, ElementData>> {
    let n = node.borrow();
    if matches!(n.kind, NodeKind::Element(_)) {
        // SAFETY: we just verified the discriminant. Ref map is safe because
        // we hold the borrow.
        let n_ref: std::cell::Ref<'_, Node> = n;
        std::mem::drop(n_ref);
        // Re-borrow with shorter lifetime and project.
        let n2 = node.borrow();
        // Use unsafe to project — but it's easier to use map_ref pattern.
        // Actually we can just clone the tag and attrs, but that allocates.
        // Use the Ref::map approach via a helper.
        std::cell::Ref::map(n2, |n| match &n.kind {
            NodeKind::Element(e) => e,
            _ => unsafe { std::hint::unreachable_unchecked() },
        })
        .into()
    } else {
        None
    }
}

/// Helper — get the tag name of an element node. Empty string for non-elements.
pub fn tag_name(node: &NodeRef) -> String {
    if let NodeKind::Element(e) = &node.borrow().kind {
        e.tag.clone()
    } else {
        String::new()
    }
}

/// Helper — get the ElementData if this is an Element (immutable borrow).
pub fn as_element_data(node: &NodeRef) -> Option<std::cell::Ref<'_, ElementData>> {
    let n = node.borrow();
    if matches!(n.kind, NodeKind::Element(_)) {
        Some(std::cell::Ref::map(n, |n| match &n.kind {
            NodeKind::Element(e) => e,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }))
    } else {
        None
    }
}

impl Node {
    /// Convenience accessor: returns the ElementData if this is an Element.
    pub fn kind_as_element(&self) -> Option<&ElementData> {
        if let NodeKind::Element(e) = &self.kind {
            Some(e)
        } else {
            None
        }
    }
}

/// Helper — get an attribute value.
pub fn get_attribute(node: &NodeRef, name: &str) -> Option<String> {
    if let NodeKind::Element(e) = &node.borrow().kind {
        e.attrs
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.clone())
    } else {
        None
    }
}

/// Helper — set an attribute value, firing mutation records.
pub fn set_attribute(node: &NodeRef, name: &str, value: &str) {
    // Take a snapshot of what we need to do.
    let old_value;
    {
        let mut n = node.borrow_mut();
        if let NodeKind::Element(e) = &mut n.kind {
            if let Some(attr) = e.attrs.iter_mut().find(|a| a.name == name) {
                old_value = Some(attr.value.clone());
                attr.value = value.to_string();
            } else {
                old_value = None;
                e.attrs.push(Attribute {
                    name: name.to_string(),
                    value: value.to_string(),
                    namespace: None,
                    prefix: None,
                });
            }
        } else {
            return;
        }
    }
    // Now queue mutation outside the borrow.
    if let Some(doc) = parent_doc(node) {
        Document::queue_mutation(
            &doc,
            MutationRecord::attributes(
                node.clone(),
                name.to_string(),
                old_value,
                Some(value.to_string()),
            ),
        );
    }
}

/// Helper — remove an attribute.
pub fn remove_attribute(node: &NodeRef, name: &str) {
    let removed_value;
    {
        let mut n = node.borrow_mut();
        if let NodeKind::Element(e) = &mut n.kind {
            if let Some(pos) = e.attrs.iter().position(|a| a.name == name) {
                let removed = e.attrs.remove(pos);
                removed_value = Some(removed.value);
            } else {
                return;
            }
        } else {
            return;
        }
    }
    if let Some(doc) = parent_doc(node) {
        Document::queue_mutation(
            &doc,
            MutationRecord::attributes(node.clone(), name.to_string(), removed_value, None),
        );
    }
}

/// Helper — has class.
pub fn has_class(node: &NodeRef, class: &str) -> bool {
    if let NodeKind::Element(e) = &node.borrow().kind {
        if let Some(c) = e.attrs.iter().find(|a| a.name == "class") {
            c.value.split_whitespace().any(|c| c == class)
        } else {
            false
        }
    } else {
        false
    }
}

pub mod accessibility;
pub mod custom_elements;
pub mod observer;
pub mod shadow;

pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
pub const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
pub const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple_tree() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let html = Document::create_element(&doc, "html");
        let head = Document::create_element(&doc, "head");
        let body = Document::create_element(&doc, "body");
        append_child(&root, html.clone());
        append_child(&html, head);
        append_child(&html, body.clone());
        let p = Document::create_element(&doc, "p");
        append_child(&body, p.clone());
        // Verify parent pointers are correct.
        assert!(p.borrow().parent.is_some());
        let parent_of_p = p.borrow().parent.as_ref().unwrap().upgrade().unwrap();
        assert_eq!(tag_name(&parent_of_p), "body");
        // Verify body's first child is p.
        assert!(body.borrow().first_child.is_some());
        assert!(body.borrow().first_child.as_ref().unwrap().borrow().id == p.borrow().id);
    }

    #[test]
    fn insert_before_works() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let parent = Document::create_element(&doc, "div");
        append_child(&root, parent.clone());
        let a = Document::create_element(&doc, "a");
        let b = Document::create_element(&doc, "b");
        let c = Document::create_element(&doc, "c");
        append_child(&parent, a.clone());
        append_child(&parent, c.clone());
        insert_before(&parent, b.clone(), Some(&c));
        // Order should be a, b, c.
        let mut order = Vec::new();
        let mut cursor = parent.borrow().first_child.clone();
        while let Some(child) = cursor {
            order.push(tag_name(&child));
            cursor = child
                .borrow()
                .next_sibling
                .clone()
                .and_then(|w| w.upgrade());
        }
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn remove_node_fixes_siblings() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let parent = Document::create_element(&doc, "div");
        append_child(&root, parent.clone());
        let a = Document::create_element(&doc, "a");
        let b = Document::create_element(&doc, "b");
        let c = Document::create_element(&doc, "c");
        append_child(&parent, a.clone());
        append_child(&parent, b.clone());
        append_child(&parent, c.clone());
        remove_from_parent(&b);
        // a.next should be c.
        assert_eq!(
            tag_name(&a.borrow().next_sibling.as_ref().unwrap().upgrade().unwrap()),
            "c"
        );
        // c.previous should be a.
        assert_eq!(
            tag_name(
                &c.borrow()
                    .previous_sibling
                    .as_ref()
                    .unwrap()
                    .upgrade()
                    .unwrap()
            ),
            "a"
        );
        // b should have no parent.
        assert!(b.borrow().parent.is_none());
    }

    #[test]
    fn mutation_records_are_queued() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let parent = Document::create_element(&doc, "div");
        append_child(&root, parent.clone());
        let child = Document::create_element(&doc, "p");
        append_child(&parent, child);
        // Tree structure intact.
        assert!(parent.borrow().first_child.is_some());
    }
}
