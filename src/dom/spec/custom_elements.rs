//! Custom Elements registry — `customElements.define()` / `customElements.get()`.
//!
//! Spec: https://html.spec.whatwg.org/multipage/custom-elements.html
//!
//! Tracks user-defined element constructors and orchestrates the upgrade
//! process when an element matching a registered name is inserted into the
//! document. Connected to the DOM through `Document::create_element()` —
//! when a custom element is created, the registry's upgrade hook fires.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::dom::spec::{NodeKind, NodeRef};

/// A registered custom element definition.
#[derive(Clone)]
pub struct CustomElementDefinition {
    /// The tag name (must contain a hyphen, e.g. "my-element").
    pub name: String,
    /// The class constructor (opaque from this side of the FFI).
    /// Stored as a string ID; the JS layer looks it up.
    pub constructor_id: String,
    /// `observedAttributes` — list of attribute names that trigger
    /// `attributeChangedCallback`.
    pub observed_attributes: Vec<String>,
    /// Whether this is an autonomous custom element (`extends` was unset)
    /// or a customized built-in (`extends="button"`, etc.).
    pub extends: Option<String>,
    /// Lifecycle callbacks (identified by string IDs to avoid an FFI).
    pub connected_callback: Option<String>,
    pub disconnected_callback: Option<String>,
    pub adopted_callback: Option<String>,
    pub attribute_changed_callback: Option<String>,
    pub form_associated_callback: Option<String>,
    pub form_disabled_callback: Option<String>,
    pub form_reset_callback: Option<String>,
    pub form_state_restore_callback: Option<String>,
}

impl std::fmt::Debug for CustomElementDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomElementDefinition")
            .field("name", &self.name)
            .field("extends", &self.extends)
            .field("observed_attributes", &self.observed_attributes)
            .finish()
    }
}

/// The custom element registry. Lives in the JS realm (one per realm),
/// but we keep a parallel struct here for the DOM side to query.
#[derive(Default)]
pub struct CustomElementRegistry {
    /// Map from tag name → definition.
    definitions: HashMap<String, CustomElementDefinition>,
    /// When `:defined` pseudo-class queries hit, this is checked.
    /// Pending upgrades: tag names seen before `define` was called.
    pending: HashMap<String, Vec<NodeRef>>,
    /// Counter for generating constructor IDs.
    next_ctor_id: u64,
}

impl CustomElementRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            pending: HashMap::new(),
            next_ctor_id: 0,
        }
    }

    /// Register a new custom element. Returns the constructor ID assigned.
    /// Spec: `customElements.define(name, constructor, options)`.
    pub fn define(
        &mut self,
        name: &str,
        extends: Option<&str>,
        observed_attributes: Vec<String>,
        callbacks: CustomElementCallbacks,
    ) -> Result<String, DefineError> {
        // Validate name: must be a valid custom element name (contain a hyphen,
        // not start with a digit, etc.).
        validate_name(name)?;
        // Must not be already defined.
        if self.definitions.contains_key(name) {
            return Err(DefineError::AlreadyDefined(name.to_string()));
        }
        let ctor_id = format!("custom-ctor-{}", self.next_ctor_id);
        self.next_ctor_id += 1;
        let def = CustomElementDefinition {
            name: name.to_string(),
            constructor_id: ctor_id.clone(),
            observed_attributes,
            extends: extends.map(|s| s.to_string()),
            connected_callback: callbacks.connected,
            disconnected_callback: callbacks.disconnected,
            adopted_callback: callbacks.adopted,
            attribute_changed_callback: callbacks.attribute_changed,
            form_associated_callback: callbacks.form_associated,
            form_disabled_callback: callbacks.form_disabled,
            form_reset_callback: callbacks.form_reset,
            form_state_restore_callback: callbacks.form_state_restore,
        };
        self.definitions.insert(name.to_string(), def);
        // Try to upgrade any pending elements.
        if let Some(pending) = self.pending.remove(name) {
            for node in pending {
                self.upgrade(&node);
            }
        }
        Ok(ctor_id)
    }

    /// Get a definition by name.
    pub fn get(&self, name: &str) -> Option<&CustomElementDefinition> {
        self.definitions.get(name)
    }

    /// Look up the definition for a given element. Considers `extends` —
    /// if the element's `is` attribute is set, we look up the customized
    /// built-in definition.
    pub fn definition_for(&self, element: &NodeRef) -> Option<&CustomElementDefinition> {
        if let NodeKind::Element(e) = &element.borrow().kind {
            // First check the `is` attribute for customized built-ins.
            if let Some(is) = e.attrs.iter().find(|a| a.name == "is") {
                return self.definitions.get(&is.value);
            }
            // Otherwise look up by tag name.
            return self.definitions.get(&e.tag);
        }
        None
    }

    /// Try to upgrade an element to a custom element. Called when:
    /// 1. An element is created via `document.createElement(name)` where
    ///    `name` matches a registered definition.
    /// 2. An element is parsed from HTML and a definition is later registered.
    pub fn upgrade(&mut self, element: &NodeRef) {
        let name = if let NodeKind::Element(e) = &element.borrow().kind {
            e.tag.clone()
        } else {
            return;
        };
        if let Some(def) = self.definitions.get(&name) {
            // Mark as upgraded.
            if let NodeKind::Element(ref mut e) = &mut element.borrow_mut().kind {
                e.custom = crate::dom::spec::CustomElementState::Custom;
            }
            // The actual constructor call happens in the JS layer — we just
            // mark the element as upgraded here. The JS bridge will detect
            // the state transition and invoke the constructor.
            let _ = def;
        } else {
            // No definition yet — add to pending.
            self.pending.entry(name).or_default().push(element.clone());
        }
    }

    /// Check if a name has been registered as a custom element.
    pub fn is_defined(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }
}

/// Lifecycle callbacks. Each is a string ID that the JS layer can use to
/// look up the actual function.
#[derive(Default, Clone)]
pub struct CustomElementCallbacks {
    pub connected: Option<String>,
    pub disconnected: Option<String>,
    pub adopted: Option<String>,
    pub attribute_changed: Option<String>,
    pub form_associated: Option<String>,
    pub form_disabled: Option<String>,
    pub form_reset: Option<String>,
    pub form_state_restore: Option<String>,
}

/// Errors that can occur during `define()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefineError {
    /// Name doesn't follow the custom element name rules.
    InvalidName(String),
    /// Name is already registered.
    AlreadyDefined(String),
    /// Constructor is already in use for another name.
    DuplicateConstructor,
}

/// Validate a custom element name. Spec: must match
/// `[a-z][a-z0-9-]*-[a-z0-9-]*` and not be a reserved name.
fn validate_name(name: &str) -> Result<(), DefineError> {
    if name.is_empty() {
        return Err(DefineError::InvalidName(name.to_string()));
    }
    // Must contain a hyphen.
    if !name.contains('-') {
        return Err(DefineError::InvalidName(format!(
            "{}: must contain a hyphen",
            name
        )));
    }
    // Must start with ASCII lowercase.
    if !name.chars().next().unwrap().is_ascii_lowercase() {
        return Err(DefineError::InvalidName(format!(
            "{}: must start with lowercase ASCII",
            name
        )));
    }
    // All chars must be ASCII lowercase alphanumeric or hyphen.
    for c in name.chars() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(DefineError::InvalidName(format!(
                "{}: invalid character '{}'",
                name, c
            )));
        }
    }
    // Reserved names.
    let reserved = [
        "annotation-xml",
        "color-profile",
        "font-face",
        "font-face-src",
        "font-face-uri",
        "font-face-format",
        "font-face-name",
        "missing-glyph",
    ];
    if reserved.contains(&name) {
        return Err(DefineError::InvalidName(format!("{}: reserved name", name)));
    }
    Ok(())
}

/// A handle to the registry for sharing across the JS bridge.
pub type RegistryHandle = Rc<RefCell<CustomElementRegistry>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::spec::{append_child, Document};

    #[test]
    fn defines_simple_custom_element() {
        let mut reg = CustomElementRegistry::new();
        let result = reg.define(
            "my-element",
            None,
            vec![],
            CustomElementCallbacks::default(),
        );
        assert!(result.is_ok());
        assert!(reg.is_defined("my-element"));
    }

    #[test]
    fn rejects_invalid_names() {
        let mut reg = CustomElementRegistry::new();
        // No hyphen.
        assert_eq!(
            reg.define("myelement", None, vec![], CustomElementCallbacks::default()),
            Err(DefineError::InvalidName(
                "myelement: must contain a hyphen".into()
            ))
        );
        // Starts with uppercase.
        assert!(reg
            .define(
                "My-Element",
                None,
                vec![],
                CustomElementCallbacks::default()
            )
            .is_err());
        // Reserved.
        assert!(reg
            .define(
                "annotation-xml",
                None,
                vec![],
                CustomElementCallbacks::default()
            )
            .is_err());
    }

    #[test]
    fn rejects_duplicate_definition() {
        let mut reg = CustomElementRegistry::new();
        reg.define(
            "my-element",
            None,
            vec![],
            CustomElementCallbacks::default(),
        )
        .unwrap();
        let result = reg.define(
            "my-element",
            None,
            vec![],
            CustomElementCallbacks::default(),
        );
        assert_eq!(
            result,
            Err(DefineError::AlreadyDefined("my-element".into()))
        );
    }

    #[test]
    fn upgrades_pending_element() {
        let doc = Document::create();
        let root = doc.borrow().root.clone();
        let el = Document::create_element(&doc, "my-widget");
        append_child(&root, el.clone());

        let mut reg = CustomElementRegistry::new();
        // Try to upgrade before definition — should add to pending.
        reg.upgrade(&el);
        assert_eq!(
            el.borrow()
                .kind_as_element()
                .map(|e| e.custom.clone())
                .unwrap_or_default(),
            crate::dom::spec::CustomElementState::Undefined
        );

        // Now define — pending should be upgraded.
        reg.define("my-widget", None, vec![], CustomElementCallbacks::default())
            .unwrap();
        assert_eq!(
            el.borrow()
                .kind_as_element()
                .map(|e| e.custom.clone())
                .unwrap_or_default(),
            crate::dom::spec::CustomElementState::Custom
        );
    }

    #[test]
    fn lookup_by_is_attribute() {
        let doc = Document::create();
        let button = Document::create_element(&doc, "button");
        crate::dom::spec::set_attribute(&button, "is", "my-button");

        let mut reg = CustomElementRegistry::new();
        reg.define(
            "my-button",
            Some("button"),
            vec![],
            CustomElementCallbacks::default(),
        )
        .unwrap();

        let def = reg.definition_for(&button);
        assert!(def.is_some());
        assert_eq!(def.unwrap().extends.as_deref(), Some("button"));
    }
}
