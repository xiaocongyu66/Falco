//! Security & Architecture — multi-process, sandboxing, SOP, site isolation,
//! crash recovery, CSP, cert validation, permissions, extensions, DevTools.
//!
//! This module groups the security-sensitive subsystems of the browser.
//! Each submodule is self-contained and can be tested in isolation.
//!
//! ## Threat Model
//!
//! The browser must defend against:
//! * **Compromised renderer** — a renderer that has been exploited (e.g.
//!   via a memory-safety bug in V8/Blink). Sandbox limits damage.
//! * **Malicious origin** — a site that tries to read another site's data.
//!   Same-Origin Policy blocks this.
//! * **Cross-site scripting (XSS)** — untrusted data injected into a page.
//!   CSP limits what scripts can run.
//! * **MITM attack** — attacker intercepts TLS traffic. Certificate
//!   validation prevents this.
//! * **Permission abuse** — a site requests camera/mic without consent.
//!   Permission model requires user approval.

pub mod cert;
pub mod csp;
pub mod devtools;
pub mod extensions;
pub mod origin;
pub mod permissions;
pub mod process;
pub mod sandbox;

pub use cert::{
    check_ct, check_ocsp, validate_chain, CertError, Certificate, CtLog, PinStore, TrustStore,
};
pub use csp::{
    check_javascript_url, escape_html, is_safe_attribute, is_safe_for_inner_html, CspPolicy,
    CspViolation, CspViolationReport,
};
pub use devtools::{
    ConsoleMessage, DevToolsServer, Event, RemoteObject, Request, Response, RpcError,
};
pub use extensions::{
    match_pattern, ActionConfig, BackgroundConfig, ContentScriptConfig, Extension,
    ExtensionManifest, ExtensionRegistry,
};
pub use origin::{
    check_cors, check_navigation, check_same_origin, registrable_domain, Origin, SopError,
};
pub use permissions::{
    IframeAllow, Permission, PermissionEntry, PermissionRegistry, PermissionSource, PermissionState,
};
pub use process::{
    should_share_process, CrashAction, CrashPolicy, Process, ProcessId, ProcessKind,
    ProcessManager, SiteIsolationPolicy,
};
pub use sandbox::{apply_sandbox, describe_active_sandbox, SandboxConfig};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// End-to-end test: an attacker origin tries to access a victim origin's
    /// DOM, and is blocked by SOP.
    #[test]
    fn sop_blocks_cross_origin_dom_access() {
        let victim = Origin::parse("https://victim.com");
        let attacker = Origin::parse("https://attacker.com");
        // Attacker script tries to read victim's DOM.
        let result = check_same_origin(&attacker, &victim);
        assert_eq!(result, Err(SopError::CrossOrigin));
    }

    /// Site isolation assigns different processes to different origins,
    /// providing defense-in-depth against renderer exploits.
    #[test]
    fn site_isolation_assigns_separate_processes() {
        let mut pm = ProcessManager::new();
        let a = Origin::parse("https://a.example.com");
        let b = Origin::parse("https://b.example.com");
        let pid_a = pm.get_or_create_renderer(&a).unwrap();
        let pid_b = pm.get_or_create_renderer(&b).unwrap();
        assert_ne!(pid_a, pid_b);
    }

    /// CSP blocks a `javascript:` URL when script-src does not include
    /// 'unsafe-inline'.
    #[test]
    fn csp_blocks_javascript_url() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("script-src 'self'");
        let result = check_javascript_url("javascript:alert(1)", &p, &page);
        assert!(result.is_err());
    }

    /// CSP allows a `javascript:` URL when script-src includes 'unsafe-inline'.
    #[test]
    fn csp_allows_javascript_url_with_unsafe_inline() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("script-src 'unsafe-inline'");
        let result = check_javascript_url("javascript:alert(1)", &p, &page);
        assert!(result.is_ok());
    }

    /// is_safe_attribute blocks event handlers and javascript: URLs in href.
    #[test]
    fn safe_attribute_blocks_xss_vectors() {
        assert!(!is_safe_attribute("div", "onclick", "alert(1)"));
        assert!(!is_safe_attribute("a", "href", "javascript:alert(1)"));
        assert!(is_safe_attribute("a", "href", "https://safe.com"));
    }

    /// A certificate with the wrong hostname should be rejected.
    #[test]
    fn cert_validation_rejects_wrong_hostname() {
        let leaf = cert::Certificate {
            subject: "CN=example.com".into(),
            issuer: "CN=Example CA".into(),
            not_before: 1_000_000_000,
            not_after: 2_000_000_000,
            subject_alt_names: vec!["example.com".into()],
            common_name: Some("example.com".into()),
            der_bytes: vec![],
            serial: vec![1],
            is_ca: false,
            path_len: None,
            ext_key_usage: vec!["serverAuth".into()],
            ocsp_responder_urls: vec![],
        };
        let mut store = cert::TrustStore::new();
        store.add_root(vec![]); // placeholder DER for root
        let chain = vec![leaf];
        let result = validate_chain(&chain, "other.com", 1_500_000_000, &store);
        assert_eq!(result, Err(CertError::HostnameMismatch));
    }

    /// A geolocation permission request triggers a prompt when the state
    /// is Prompt.
    #[test]
    fn permission_request_triggers_prompt() {
        let registry = permissions::PermissionRegistry::new();
        registry.register_prompt_handler(|_, _| PermissionState::Granted);
        let mut registry = registry;
        let origin = Origin::parse("https://example.com");
        let result = registry.request(&origin, Permission::Geolocation, 1000);
        assert_eq!(result, PermissionState::Granted);
    }

    /// An extension's content script only runs on matching URLs.
    #[test]
    fn content_script_only_runs_on_matching_urls() {
        let mut reg = extensions::ExtensionRegistry::new();
        let manifest = extensions::ExtensionManifest {
            manifest_version: 3,
            name: "AdBlock".into(),
            version: "1.0".into(),
            description: None,
            background: None,
            content_scripts: vec![ContentScriptConfig {
                matches: vec!["*://*.example.com/*".into()],
                exclude_matches: vec![],
                css: vec![],
                js: vec!["adblock.js".into()],
                run_at: "document_idle".into(),
                world: "ISOLATED".into(),
            }],
            permissions: vec![],
            host_permissions: vec![],
            web_accessible_resources: vec![],
            action: None,
            icons: std::collections::HashMap::new(),
            content_security_policy: None,
        };
        reg.install(manifest, std::path::PathBuf::from("/tmp/adblock"));
        assert_eq!(
            reg.content_scripts_for_url("https://www.example.com/page")
                .len(),
            1
        );
        assert_eq!(
            reg.content_scripts_for_url("https://other.com/page").len(),
            0
        );
    }

    /// DevTools protocol rejects requests until enabled.
    #[test]
    fn devtools_requires_enable_first() {
        let mut s = DevToolsServer::new();
        let req = Request {
            id: 1,
            method: "Page.navigate".into(),
            params: devtools::serde_json_lite::Value::Null,
        };
        let resp = s.handle_request(req);
        assert!(resp.error.is_some());
        s.enable();
        let req2 = Request {
            id: 2,
            method: "Page.navigate".into(),
            params: devtools::serde_json_lite::Value::Object({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "url".to_string(),
                    devtools::serde_json_lite::Value::String("https://x.com".into()),
                );
                m
            }),
        };
        let resp2 = s.handle_request(req2);
        assert!(resp2.result.is_some());
    }

    /// Crash recovery restarts the crashed process with the same origin.
    #[test]
    fn crash_recovery_restarts_process() {
        let mut pm = ProcessManager::new();
        pm.crash_policy = CrashPolicy::Restart { max_restarts: 3 };
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        let action = pm.handle_crash(pid);
        match action {
            CrashAction::Restarted { old_pid, new_pid } => {
                assert_eq!(old_pid, pid);
                assert_ne!(new_pid, pid);
            }
            _ => panic!("expected restart"),
        }
    }
}
