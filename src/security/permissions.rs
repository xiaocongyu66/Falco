//! Permission model — camera, microphone, geolocation, notifications, etc.
//!
//! Spec: https://w3c.github.io/permissions/
//!
//! Each permission has a state: `granted`, `denied`, or `prompt`. When JS
//! requests a permission (e.g. via `navigator.geolocation.getCurrentPosition`),
//! the browser:
//! 1. Checks the current state.
//! 2. If `prompt`, shows a UI asking the user.
//! 3. Records the user's decision (per-origin, persistent across sessions).

use crate::security::origin::Origin;
use std::cell::RefCell;
use std::collections::HashMap;

/// A permission name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    Geolocation,
    Camera,
    Microphone,
    Notifications,
    ClipboardRead,
    ClipboardWrite,
    PersistentStorage,
    Midi,
    BackgroundSync,
    Bluetooth,
    Usb,
    Serial,
    Hid,
    Vr,
    Ar,
    PaymentHandler,
    IdleDetection,
    LocalFonts,
    StorageAccess,
    WindowManagement,
}

impl Permission {
    /// Get the spec name as a string (e.g. "geolocation").
    pub fn name(&self) -> &'static str {
        match self {
            Self::Geolocation => "geolocation",
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Notifications => "notifications",
            Self::ClipboardRead => "clipboard-read",
            Self::ClipboardWrite => "clipboard-write",
            Self::PersistentStorage => "persistent-storage",
            Self::Midi => "midi",
            Self::BackgroundSync => "background-sync",
            Self::Bluetooth => "bluetooth",
            Self::Usb => "usb",
            Self::Serial => "serial",
            Self::Hid => "hid",
            Self::Vr => "vr",
            Self::Ar => "ar",
            Self::PaymentHandler => "payment-handler",
            Self::IdleDetection => "idle-detection",
            Self::LocalFonts => "local-fonts",
            Self::StorageAccess => "storage-access",
            Self::WindowManagement => "window-management",
        }
    }

    /// Parse a permission name string.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "geolocation" => Some(Self::Geolocation),
            "camera" => Some(Self::Camera),
            "microphone" => Some(Self::Microphone),
            "notifications" => Some(Self::Notifications),
            "clipboard-read" => Some(Self::ClipboardRead),
            "clipboard-write" => Some(Self::ClipboardWrite),
            "persistent-storage" => Some(Self::PersistentStorage),
            "midi" => Some(Self::Midi),
            "background-sync" => Some(Self::BackgroundSync),
            "bluetooth" => Some(Self::Bluetooth),
            "usb" => Some(Self::Usb),
            "serial" => Some(Self::Serial),
            "hid" => Some(Self::Hid),
            "vr" => Some(Self::Vr),
            "ar" => Some(Self::Ar),
            "payment-handler" => Some(Self::PaymentHandler),
            "idle-detection" => Some(Self::IdleDetection),
            "local-fonts" => Some(Self::LocalFonts),
            "storage-access" => Some(Self::StorageAccess),
            "window-management" => Some(Self::WindowManagement),
            _ => None,
        }
    }
}

/// The state of a permission for a given origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    /// User has explicitly granted.
    Granted,
    /// User has explicitly denied.
    Denied,
    /// Browser should ask the user.
    Prompt,
}

/// How the permission was granted/denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionSource {
    /// User clicked "Allow" / "Block" in the prompt.
    UserDecision,
    /// Set via the permissions settings page.
    Settings,
    /// Granted by default (e.g. some permissions are auto-granted on https).
    Default,
    /// Granted via `<iframe allow="...">` attribute.
    IframeAllow,
}

/// A stored permission decision.
#[derive(Debug, Clone)]
pub struct PermissionEntry {
    pub state: PermissionState,
    pub source: PermissionSource,
    /// When the decision was made (Unix epoch).
    pub decided_at: u64,
}

/// The permission store — per-origin, per-permission.
pub struct PermissionRegistry {
    /// Map from (origin, permission) → entry.
    entries: HashMap<(String, Permission), PermissionEntry>,
    /// Callbacks to invoke when a permission prompt is shown.
    /// The callback should return the user's decision.
    prompt_handlers: RefCell<Vec<Box<dyn Fn(&Origin, Permission) -> PermissionState>>>,
}

impl std::fmt::Debug for PermissionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionRegistry")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl Default for PermissionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            prompt_handlers: RefCell::new(Vec::new()),
        }
    }

    /// Get the current state of a permission for an origin.
    pub fn get(&self, origin: &Origin, perm: Permission) -> PermissionState {
        let key = (origin.serialize(), perm);
        match self.entries.get(&key) {
            Some(entry) => entry.state,
            None => PermissionState::Prompt,
        }
    }

    /// Set the permission state for an origin.
    pub fn set(
        &mut self,
        origin: &Origin,
        perm: Permission,
        state: PermissionState,
        source: PermissionSource,
        now: u64,
    ) {
        let key = (origin.serialize(), perm);
        self.entries.insert(
            key,
            PermissionEntry {
                state,
                source,
                decided_at: now,
            },
        );
    }

    /// Request a permission. If the state is `Prompt`, fires the registered
    /// prompt handlers to ask the user.
    pub fn request(&mut self, origin: &Origin, perm: Permission, now: u64) -> PermissionState {
        let current = self.get(origin, perm);
        match current {
            PermissionState::Granted | PermissionState::Denied => current,
            PermissionState::Prompt => {
                // Run prompt handlers in order until one returns a non-Prompt state.
                let mut decision = PermissionState::Prompt;
                // Iterate by index to avoid cloning the Box.
                let handlers_len = self.prompt_handlers.borrow().len();
                for i in 0..handlers_len {
                    // Take a raw pointer to the handler so we can call it without
                    // holding a borrow on the RefCell.
                    let handler_ptr: *const Box<dyn Fn(&Origin, Permission) -> PermissionState> =
                        &self.prompt_handlers.borrow()[i];
                    // SAFETY: we hold a borrow of self (via &mut self), and we don't
                    // mutate prompt_handlers during the loop.
                    let result = unsafe { (*handler_ptr)(origin, perm) };
                    if result != PermissionState::Prompt {
                        decision = result;
                        break;
                    }
                }
                if decision != PermissionState::Prompt {
                    self.set(origin, perm, decision, PermissionSource::UserDecision, now);
                }
                decision
            }
        }
    }

    /// Reset a permission to Prompt (used by the settings UI).
    pub fn reset(&mut self, origin: &Origin, perm: Permission) {
        let key = (origin.serialize(), perm);
        self.entries.remove(&key);
    }

    /// Reset all permissions for an origin (used when clearing site data).
    pub fn reset_all_for_origin(&mut self, origin: &Origin) {
        let prefix = origin.serialize();
        self.entries.retain(|(o, _), _| o != &prefix);
    }

    /// Register a prompt handler. Called when a permission needs to be asked.
    pub fn register_prompt_handler<F: Fn(&Origin, Permission) -> PermissionState + 'static>(
        &self,
        handler: F,
    ) {
        self.prompt_handlers.borrow_mut().push(Box::new(handler));
    }

    /// List all permissions for an origin (for the settings UI).
    pub fn list_for_origin(&self, origin: &Origin) -> Vec<(Permission, PermissionState)> {
        let prefix = origin.serialize();
        self.entries
            .iter()
            .filter(|((o, _), _)| o == &prefix)
            .map(|((_, p), e)| (*p, e.state))
            .collect()
    }
}

/// An `<iframe allow="...">` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IframeAllow {
    pub permissions: Vec<(Permission, Option<String>)>,
}

impl IframeAllow {
    /// Parse an `allow` attribute value.
    /// Examples:
    ///   `allow="camera; microphone"`
    ///   `allow="geolocation *"`
    ///   `allow="camera https://a.com https://b.com"`
    pub fn parse(s: &str) -> Self {
        let mut perms = Vec::new();
        for token in s.split(';') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let mut parts = token.split_whitespace();
            if let Some(name) = parts.next() {
                if let Some(p) = Permission::from_name(name) {
                    let origins: Vec<String> = parts.map(|s| s.to_string()).collect();
                    let origin_filter = if origins.is_empty() || origins.iter().any(|o| o == "*") {
                        None
                    } else {
                        Some(origins.join(","))
                    };
                    perms.push((p, origin_filter));
                }
            }
        }
        Self { permissions: perms }
    }

    /// Check if a permission is allowed for a given origin.
    pub fn allows(&self, perm: Permission, origin: &Origin) -> bool {
        for (p, filter) in &self.permissions {
            if *p == perm {
                match filter {
                    None => return true, // no origin filter — allow all
                    Some(origins) => {
                        for o in origins.split(',') {
                            let parsed = Origin::parse(o.trim());
                            if parsed.is_same_origin(origin) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_name_roundtrip() {
        for p in &[
            Permission::Geolocation,
            Permission::Camera,
            Permission::Microphone,
            Permission::Notifications,
            Permission::Bluetooth,
        ] {
            let name = p.name();
            assert_eq!(Permission::from_name(name), Some(*p));
        }
    }

    #[test]
    fn default_state_is_prompt() {
        let reg = PermissionRegistry::new();
        let origin = Origin::parse("https://example.com");
        assert_eq!(
            reg.get(&origin, Permission::Camera),
            PermissionState::Prompt
        );
    }

    #[test]
    fn set_and_get_permission() {
        let mut reg = PermissionRegistry::new();
        let origin = Origin::parse("https://example.com");
        reg.set(
            &origin,
            Permission::Geolocation,
            PermissionState::Granted,
            PermissionSource::UserDecision,
            1000,
        );
        assert_eq!(
            reg.get(&origin, Permission::Geolocation),
            PermissionState::Granted
        );
    }

    #[test]
    fn request_returns_existing_decision() {
        let mut reg = PermissionRegistry::new();
        let origin = Origin::parse("https://example.com");
        reg.set(
            &origin,
            Permission::Camera,
            PermissionState::Denied,
            PermissionSource::UserDecision,
            1000,
        );
        // Request should return Denied without prompting.
        let result = reg.request(&origin, Permission::Camera, 2000);
        assert_eq!(result, PermissionState::Denied);
    }

    #[test]
    fn request_prompts_when_prompt() {
        let reg = PermissionRegistry::new();
        reg.register_prompt_handler(|_o, _p| PermissionState::Granted);
        let mut reg = reg;
        let origin = Origin::parse("https://example.com");
        let result = reg.request(&origin, Permission::Geolocation, 1000);
        assert_eq!(result, PermissionState::Granted);
        // Subsequent request should return Granted without prompting.
        let result = reg.request(&origin, Permission::Geolocation, 2000);
        assert_eq!(result, PermissionState::Granted);
    }

    #[test]
    fn reset_permission() {
        let mut reg = PermissionRegistry::new();
        let origin = Origin::parse("https://example.com");
        reg.set(
            &origin,
            Permission::Camera,
            PermissionState::Granted,
            PermissionSource::UserDecision,
            1000,
        );
        reg.reset(&origin, Permission::Camera);
        assert_eq!(
            reg.get(&origin, Permission::Camera),
            PermissionState::Prompt
        );
    }

    #[test]
    fn reset_all_for_origin() {
        let mut reg = PermissionRegistry::new();
        let a = Origin::parse("https://a.com");
        let b = Origin::parse("https://b.com");
        reg.set(
            &a,
            Permission::Camera,
            PermissionState::Granted,
            PermissionSource::UserDecision,
            1000,
        );
        reg.set(
            &b,
            Permission::Camera,
            PermissionState::Denied,
            PermissionSource::UserDecision,
            1000,
        );
        reg.reset_all_for_origin(&a);
        assert_eq!(reg.get(&a, Permission::Camera), PermissionState::Prompt);
        assert_eq!(reg.get(&b, Permission::Camera), PermissionState::Denied);
    }

    #[test]
    fn iframe_allow_simple() {
        let a = IframeAllow::parse("camera; microphone");
        let o = Origin::parse("https://example.com");
        assert!(a.allows(Permission::Camera, &o));
        assert!(a.allows(Permission::Microphone, &o));
        assert!(!a.allows(Permission::Geolocation, &o));
    }

    #[test]
    fn iframe_allow_with_origin_filter() {
        let a = IframeAllow::parse("camera https://a.com https://b.com");
        let a_origin = Origin::parse("https://a.com");
        let b_origin = Origin::parse("https://b.com");
        let c_origin = Origin::parse("https://c.com");
        assert!(a.allows(Permission::Camera, &a_origin));
        assert!(a.allows(Permission::Camera, &b_origin));
        assert!(!a.allows(Permission::Camera, &c_origin));
    }

    #[test]
    fn iframe_allow_wildcard() {
        let a = IframeAllow::parse("geolocation *");
        let any = Origin::parse("https://anything.com");
        assert!(a.allows(Permission::Geolocation, &any));
    }

    #[test]
    fn list_permissions_for_origin() {
        let mut reg = PermissionRegistry::new();
        let origin = Origin::parse("https://example.com");
        reg.set(
            &origin,
            Permission::Camera,
            PermissionState::Granted,
            PermissionSource::UserDecision,
            1000,
        );
        reg.set(
            &origin,
            Permission::Microphone,
            PermissionState::Denied,
            PermissionSource::UserDecision,
            1000,
        );
        let list = reg.list_for_origin(&origin);
        assert_eq!(list.len(), 2);
    }
}
