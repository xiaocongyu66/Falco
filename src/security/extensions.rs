//! Extension API — content scripts, background pages, manifest.
//!
//! Spec: https://developer.chrome.com/docs/extensions/mv3/
//!
//! Browser extensions can:
//! * Run content scripts on matching URLs (read/modify page DOM).
//! * Run a background service worker (persistent state, listen to events).
//! * Inject UI elements via the chrome.action API.
//! * Modify network requests via webRequest.
//! * Add custom CSS to pages.
//!
//! Manifest V3 (current standard):
//! * Background is a Service Worker (no persistent page).
//! * Content scripts declare match patterns and JS/CSS files.
//! * Permissions are explicit and user-grantable.

use crate::security::origin::Origin;
use std::collections::HashMap;
use std::path::PathBuf;

/// A parsed extension manifest (manifest.json).
#[derive(Debug, Clone)]
pub struct ExtensionManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub background: Option<BackgroundConfig>,
    pub content_scripts: Vec<ContentScriptConfig>,
    pub permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub web_accessible_resources: Vec<Vec<String>>,
    pub action: Option<ActionConfig>,
    pub icons: HashMap<String, PathBuf>,
    pub content_security_policy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BackgroundConfig {
    /// Service worker script (Manifest V3) — replaces persistent BG page.
    pub service_worker: String,
    pub r#type: String, // "module" or "classic"
}

#[derive(Debug, Clone)]
pub struct ContentScriptConfig {
    /// URL match patterns (e.g. ["https://*.example.com/*"]).
    pub matches: Vec<String>,
    /// URLs to exclude.
    pub exclude_matches: Vec<String>,
    /// CSS files to inject.
    pub css: Vec<String>,
    /// JS files to inject (in order).
    pub js: Vec<String>,
    /// When to inject: "document_idle", "document_start", "document_end".
    pub run_at: String,
    /// Whether the script runs in an isolated world (default true).
    pub world: String,
}

#[derive(Debug, Clone)]
pub struct ActionConfig {
    pub default_title: Option<String>,
    pub default_popup: Option<String>,
    pub default_icon: HashMap<String, PathBuf>,
}

/// A loaded extension.
#[derive(Debug, Clone)]
pub struct Extension {
    pub id: String,
    pub manifest: ExtensionManifest,
    /// Path to the extension's directory on disk.
    pub path: PathBuf,
    /// Whether the extension is enabled.
    pub enabled: bool,
}

/// The extension registry — tracks all installed extensions.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    pub extensions: Vec<Extension>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install an extension from a manifest.
    pub fn install(&mut self, manifest: ExtensionManifest, path: PathBuf) -> String {
        let id = generate_extension_id(&manifest.name);
        let ext = Extension {
            id: id.clone(),
            manifest,
            path,
            enabled: true,
        };
        self.extensions.push(ext);
        id
    }

    /// Uninstall an extension by ID.
    pub fn uninstall(&mut self, id: &str) -> bool {
        let len = self.extensions.len();
        self.extensions.retain(|e| e.id != id);
        self.extensions.len() != len
    }

    /// Enable/disable an extension.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(e) = self.extensions.iter_mut().find(|e| e.id == id) {
            e.enabled = enabled;
        }
    }

    /// Get an extension by ID.
    pub fn get(&self, id: &str) -> Option<&Extension> {
        self.extensions.iter().find(|e| e.id == id)
    }

    /// Find content scripts that should run on a given URL.
    pub fn content_scripts_for_url(&self, url: &str) -> Vec<(String, ContentScriptConfig)> {
        let mut out = Vec::new();
        for ext in &self.extensions {
            if !ext.enabled {
                continue;
            }
            for cs in &ext.manifest.content_scripts {
                if cs.matches.iter().any(|m| match_pattern(m, url))
                    && !cs.exclude_matches.iter().any(|m| match_pattern(m, url))
                {
                    out.push((ext.id.clone(), cs.clone()));
                }
            }
        }
        out
    }

    /// Check if an extension has a given permission.
    pub fn has_permission(&self, id: &str, permission: &str) -> bool {
        self.extensions
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.manifest.permissions.iter().any(|p| p == permission))
            .unwrap_or(false)
    }

    /// Check if an extension has host permission for a URL.
    pub fn has_host_permission(&self, id: &str, url: &str) -> bool {
        self.extensions
            .iter()
            .find(|e| e.id == id)
            .map(|e| {
                e.manifest
                    .host_permissions
                    .iter()
                    .any(|m| match_pattern(m, url))
            })
            .unwrap_or(false)
    }
}

/// Match a Chrome match pattern against a URL.
///
/// Patterns: `<scheme>://<host><path>`
/// - `*` matches any scheme or any path.
/// - `*.example.com` matches any subdomain.
/// - `<all_urls>` matches every URL.
pub fn match_pattern(pattern: &str, url: &str) -> bool {
    if pattern == "<all_urls>" {
        return url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("file://")
            || url.starts_with("ftp://");
    }
    // Parse pattern: scheme://host/path
    let (scheme, rest) = match pattern.find("://") {
        Some(i) => (&pattern[..i], &pattern[i + 3..]),
        None => return false,
    };
    let (host_path, _) = (rest, "");
    let (host, path) = match host_path.find('/') {
        Some(i) => (&host_path[..i], &host_path[i..]),
        None => (host_path, "/"),
    };
    // Parse URL the same way.
    let url_origin = Origin::parse(url);
    if url_origin.opaque {
        return false;
    }
    // Check scheme.
    let url_scheme = &url_origin.scheme;
    if scheme != "*" && scheme != url_scheme {
        return false;
    }
    // Check host.
    if host == "*" {
        // matches any host
    } else if host.starts_with("*.") {
        let suffix = &host[1..];
        if !url_origin.host.ends_with(suffix) && url_origin.host != host[2..] {
            return false;
        }
    } else if host != url_origin.host {
        return false;
    }
    // Check path (with * support).
    if path == "/*" || path == "/" {
        return true;
    }
    let url_path = match url.find("//") {
        Some(i) => {
            let after_scheme = &url[i + 2..];
            match after_scheme.find('/') {
                Some(j) => &after_scheme[j..],
                None => "/",
            }
        }
        None => "/",
    };
    glob_match(path, url_path)
}

fn glob_match(pattern: &str, s: &str) -> bool {
    // Simple glob matcher: * matches any sequence.
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == s;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == s;
    }
    let mut cursor = 0;
    if !s[cursor..].starts_with(parts[0]) {
        return false;
    }
    cursor += parts[0].len();
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match s[cursor..].find(part) {
            Some(i) => cursor += i + part.len(),
            None => return false,
        }
    }
    let last = parts.last().unwrap();
    s.ends_with(last) && s.len() - last.len() >= cursor
}

/// Generate an extension ID from the extension name.
/// Real Chrome uses the public key fingerprint; we use a simple hash.
fn generate_extension_id(name: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for c in name.chars() {
        hash ^= c as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Chrome extension IDs are 32 lowercase letters (a-p).
    let mut id = String::new();
    for _ in 0..32 {
        let byte = (hash & 0xF) as u8;
        id.push((b'a' + byte % 16) as char);
        hash >>= 4;
        if hash == 0 {
            hash = 0xcbf29ce484222325;
        }
    }
    id
}

/// The chrome.* API surface available to extensions.
/// We model this as an enum of method calls; the extension host dispatches.
#[derive(Debug, Clone)]
pub enum ChromeApi {
    /// chrome.tabs.query({ active: true, currentWindow: true })
    TabsQuery { query: String },
    /// chrome.tabs.sendMessage(tabId, message)
    TabsSendMessage { tab_id: u64, message: String },
    /// chrome.runtime.sendMessage(message)
    RuntimeSendMessage { message: String },
    /// chrome.storage.local.set({ key: value })
    StorageLocalSet { key: String, value: String },
    /// chrome.storage.local.get(key)
    StorageLocalGet { key: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(name: &str, matches: &[&str]) -> ExtensionManifest {
        ExtensionManifest {
            manifest_version: 3,
            name: name.into(),
            version: "1.0".into(),
            description: None,
            background: None,
            content_scripts: vec![ContentScriptConfig {
                matches: matches.iter().map(|s| s.to_string()).collect(),
                exclude_matches: vec![],
                css: vec![],
                js: vec!["content.js".into()],
                run_at: "document_idle".into(),
                world: "ISOLATED".into(),
            }],
            permissions: vec!["storage".into()],
            host_permissions: vec![],
            web_accessible_resources: vec![],
            action: None,
            icons: HashMap::new(),
            content_security_policy: None,
        }
    }

    #[test]
    fn install_extension() {
        let mut reg = ExtensionRegistry::new();
        let id = reg.install(
            make_manifest("MyExt", &["*://*.example.com/*"]),
            PathBuf::from("/tmp/ext"),
        );
        assert!(!id.is_empty());
        assert_eq!(reg.extensions.len(), 1);
    }

    #[test]
    fn uninstall_extension() {
        let mut reg = ExtensionRegistry::new();
        let id = reg.install(make_manifest("MyExt", &[]), PathBuf::from("/tmp/ext"));
        assert!(reg.uninstall(&id));
        assert_eq!(reg.extensions.len(), 0);
    }

    #[test]
    fn enable_disable() {
        let mut reg = ExtensionRegistry::new();
        let id = reg.install(make_manifest("MyExt", &[]), PathBuf::from("/tmp/ext"));
        reg.set_enabled(&id, false);
        assert!(!reg.get(&id).unwrap().enabled);
    }

    #[test]
    fn match_pattern_simple() {
        assert!(match_pattern("https://*/*", "https://example.com/page"));
        assert!(match_pattern(
            "*://*.example.com/*",
            "https://www.example.com/page"
        ));
        assert!(match_pattern(
            "*://*.example.com/*",
            "http://sub.example.com/"
        ));
        assert!(!match_pattern("*://*.example.com/*", "https://other.com/"));
    }

    #[test]
    fn match_pattern_all_urls() {
        assert!(match_pattern("<all_urls>", "https://example.com"));
        assert!(match_pattern("<all_urls>", "http://anything.com"));
        assert!(!match_pattern("<all_urls>", "data:text/html,hi"));
    }

    #[test]
    fn match_pattern_specific() {
        assert!(match_pattern(
            "https://example.com/*",
            "https://example.com/page"
        ));
        assert!(!match_pattern(
            "https://example.com/*",
            "https://other.com/page"
        ));
    }

    #[test]
    fn match_pattern_path_glob() {
        assert!(match_pattern(
            "https://example.com/articles/*",
            "https://example.com/articles/123"
        ));
        assert!(!match_pattern(
            "https://example.com/articles/*",
            "https://example.com/page"
        ));
    }

    #[test]
    fn content_scripts_for_url() {
        let mut reg = ExtensionRegistry::new();
        reg.install(
            make_manifest("Ext1", &["*://*.example.com/*"]),
            PathBuf::from("/tmp/ext1"),
        );
        reg.install(
            make_manifest("Ext2", &["*://*.other.com/*"]),
            PathBuf::from("/tmp/ext2"),
        );

        let scripts = reg.content_scripts_for_url("https://www.example.com/page");
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].0.len(), 32); // extension ID length
    }

    #[test]
    fn disabled_extension_not_returned() {
        let mut reg = ExtensionRegistry::new();
        let id = reg.install(
            make_manifest("Ext1", &["*://*.example.com/*"]),
            PathBuf::from("/tmp/ext1"),
        );
        reg.set_enabled(&id, false);
        let scripts = reg.content_scripts_for_url("https://www.example.com/page");
        assert_eq!(scripts.len(), 0);
    }

    #[test]
    fn exclude_matches_filters() {
        let mut manifest = make_manifest("Ext1", &["*://*.example.com/*"]);
        manifest.content_scripts[0].exclude_matches = vec!["*://admin.example.com/*".to_string()];
        let mut reg = ExtensionRegistry::new();
        reg.install(manifest, PathBuf::from("/tmp/ext1"));
        // Regular URL — should match.
        assert_eq!(
            reg.content_scripts_for_url("https://www.example.com/page")
                .len(),
            1
        );
        // Excluded URL — should not match.
        assert_eq!(
            reg.content_scripts_for_url("https://admin.example.com/page")
                .len(),
            0
        );
    }

    #[test]
    fn has_permission() {
        let mut reg = ExtensionRegistry::new();
        let id = reg.install(make_manifest("Ext1", &[]), PathBuf::from("/tmp/ext1"));
        assert!(reg.has_permission(&id, "storage"));
        assert!(!reg.has_permission(&id, "tabs"));
    }

    #[test]
    fn extension_id_is_32_chars() {
        let id = generate_extension_id("My Extension");
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| ('a'..='p').contains(&c)));
    }

    #[test]
    fn different_names_get_different_ids() {
        let a = generate_extension_id("Extension A");
        let b = generate_extension_id("Extension B");
        assert_ne!(a, b);
    }
}
