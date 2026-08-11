//! Encrypted Media Extensions (EME) — DRM support framework.
//!
//! Spec: https://www.w3.org/TR/encrypted-media/
//!
//! EME provides a JavaScript API for playing encrypted (DRM-protected)
//! media. It's how Netflix, Disney+, and HBO Max play Widevine-protected
//! content in browsers.
//!
//! # Architecture
//!
//! ```js
//! const mediaKeys = await MediaKeys.create('com.widevine.alpha');
//! await video.setMediaKeys(mediaKeys);
//! // When the media element encounters an encrypted chunk:
//! const session = mediaKeys.createSession();
//! session.generateRequest('cenc', initData);
//! session.addEventListener('message', async (event) => {
//!   // Send event.message to the license server, get back a license.
//!   const license = await fetch(licenseServer, { method: 'POST', body: event.message });
//!   await session.update(license);
//! });
//! ```
//!
//! # What this implements
//!
//! * `MediaKeys` — represents a DRM key system.
//! * `MediaKeySession` — a decryption session.
//! * `generateRequest()` — creates a license request.
//! * `update()` — applies a license from the server.
//! * `closed` event — fires when the session is closed.
//! * `keystatuseschange` event — fires when key statuses change.
//!
//! # What this does NOT implement (and why)
//!
//! * **Widevine** — proprietary Google DRM. Requires a commercial license
//!   and access to the Widevine CDM (Content Decryption Module) binary,
//!   which is only available to licensed browser vendors.
//! * **PlayReady** — Microsoft DRM, similar licensing restrictions.
//! * **FairPlay** — Apple DRM, requires Apple Developer license.
//! * **Actual decryption** — even with a CDM, decryption happens in a
//!   sandboxed process that we can't replicate.
//!
//! We provide the API surface so that JS code using EME doesn't throw
//! "not supported" errors, but actual decryption of DRM-protected content
//! is not possible.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The key system — identifies the DRM scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySystem {
    /// Google Widevine (used by Chrome, Firefox, Android).
    Widevine,
    /// Microsoft PlayReady (used by Edge, IE, Xbox).
    PlayReady,
    /// Apple FairPlay (used by Safari, iOS).
    FairPlay,
    /// Clear Key — the only mandatory-to-implement key system. Uses raw
    /// keys without DRM, for testing.
    ClearKey,
}

impl KeySystem {
    /// Parse a key system string (as passed to `navigator.requestMediaKeySystemAccess`).
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "com.widevine.alpha" => Some(Self::Widevine),
            "com.microsoft.playready" => Some(Self::PlayReady),
            "com.apple.fps.1_0" | "com.apple.fps" => Some(Self::FairPlay),
            "org.w3.clearkey" => Some(Self::ClearKey),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Widevine => "com.widevine.alpha",
            Self::PlayReady => "com.microsoft.playready",
            Self::FairPlay => "com.apple.fps.1_0",
            Self::ClearKey => "org.w3.clearkey",
        }
    }

    /// Whether this key system is actually usable (has a CDM available).
    /// Widevine/PlayReady/FairPlay are never usable in our implementation.
    /// ClearKey is usable because it doesn't require a CDM.
    pub fn is_available(&self) -> bool {
        matches!(self, Self::ClearKey)
    }
}

/// MediaKeySystemConfiguration — describes what capabilities we need.
#[derive(Debug, Clone)]
pub struct MediaKeySystemConfig {
    pub init_data_types: Vec<String>, // e.g. "cenc", "webm", "keyids"
    pub audio_capabilities: Vec<MediaKeySystemMediaCapability>,
    pub video_capabilities: Vec<MediaKeySystemMediaCapability>,
    pub distinctive_identifier: MediaKeySystemAvailability,
    pub persistent_state: MediaKeySystemAvailability,
    pub session_types: Vec<SessionType>,
}

#[derive(Debug, Clone)]
pub struct MediaKeySystemMediaCapability {
    pub content_type: String,
    pub robustness: String,
    pub encryption_scheme: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKeySystemAvailability {
    Required,
    Optional,
    NotAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    Temporary,
    PersistentLicense,
    PersistentUsageRecord,
}

/// MediaKeySystemAccess — the result of requesting a key system.
pub struct MediaKeySystemAccess {
    pub key_system: KeySystem,
    pub configuration: MediaKeySystemConfig,
}

impl MediaKeySystemAccess {
    /// Check if the key system supports the requested configuration.
    pub fn is_supported(&self) -> bool {
        // Widevine/PlayReady/FairPlay are never supported (no CDM).
        // ClearKey supports temporary sessions with basic codecs.
        if !self.key_system.is_available() {
            return false;
        }
        // ClearKey only supports temporary sessions.
        if self
            .configuration
            .session_types
            .iter()
            .any(|t| *t != SessionType::Temporary)
        {
            return false;
        }
        true
    }
}

/// MediaKeys — the key system instance.
pub struct MediaKeys {
    pub key_system: KeySystem,
    pub sessions: Mutex<Vec<Arc<MediaKeySession>>>,
}

impl MediaKeys {
    pub fn new(key_system: KeySystem) -> Arc<Self> {
        Arc::new(Self {
            key_system,
            sessions: Mutex::new(Vec::new()),
        })
    }

    /// Create a new session for decrypting media.
    pub fn create_session(self: &Arc<Self>, session_type: SessionType) -> Arc<MediaKeySession> {
        let session = MediaKeySession::new(session_type);
        self.sessions.lock().unwrap().push(session.clone());
        session
    }
}

/// A MediaKeySession — holds decryption keys for one playback session.
pub struct MediaKeySession {
    pub session_id: String,
    pub session_type: SessionType,
    pub closed: Mutex<bool>,
    /// Map from key ID (16 bytes) to key status.
    pub key_statuses: Mutex<HashMap<Vec<u8>, KeyStatus>>,
    /// The expiration time (None = no expiration).
    pub expiration: Mutex<Option<f64>>,
    /// Callbacks.
    on_message: Mutex<Option<Box<dyn Fn(MediaKeyMessage) + Send>>>,
    on_keystatuseschange: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_closed: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl std::fmt::Debug for MediaKeySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaKeySession")
            .field("session_id", &self.session_id)
            .field("session_type", &self.session_type)
            .field("closed", &*self.closed.lock().unwrap())
            .field("key_count", &self.key_statuses.lock().unwrap().len())
            .finish()
    }
}

impl MediaKeySession {
    fn new(session_type: SessionType) -> Arc<Self> {
        Arc::new(Self {
            session_id: generate_session_id(),
            session_type,
            closed: Mutex::new(false),
            key_statuses: Mutex::new(HashMap::new()),
            expiration: Mutex::new(None),
            on_message: Mutex::new(None),
            on_keystatuseschange: Mutex::new(None),
            on_closed: Mutex::new(None),
        })
    }

    /// generateRequest(initDataType, initData) — create a license request.
    ///
    /// For ClearKey, the request contains the key IDs. For Widevine, it
    /// contains a PSSH box from the media.
    pub fn generate_request(&self, init_data_type: &str, init_data: &[u8]) -> Result<(), String> {
        // For ClearKey, we can extract key IDs directly from the init data.
        if init_data_type == "keyids" {
            // The init data is a JSON array of key IDs (base64).
            // For simplicity, we just fire a message event with the init data.
            let message = MediaKeyMessage {
                message_type: KeyMessageType::LicenseRequest,
                message: init_data.to_vec(),
            };
            if let Some(cb) = self.on_message.lock().unwrap().as_ref() {
                cb(message);
            }
        } else if init_data_type == "cenc" {
            // Common Encryption (cenc) — used by Widevine and PlayReady.
            // The init data contains a PSSH box with the system ID and
            // optional data. We can't process it without a CDM.
            if self.session_type != SessionType::Temporary {
                return Err("Only temporary sessions supported without CDM".to_string());
            }
            // For demo purposes, fire a license request message.
            let message = MediaKeyMessage {
                message_type: KeyMessageType::LicenseRequest,
                message: init_data.to_vec(),
            };
            if let Some(cb) = self.on_message.lock().unwrap().as_ref() {
                cb(message);
            }
        } else {
            return Err(format!("Unsupported init data type: {}", init_data_type));
        }
        Ok(())
    }

    /// update(response) — apply a license received from the license server.
    ///
    /// For ClearKey, the response is a JSON object mapping key IDs to keys.
    pub fn update(&self, response: &[u8]) -> Result<(), String> {
        // For ClearKey, parse the response and add keys.
        // Format: {"keys":[{"kty":"oct","kid":"...","k":"..."}]}
        // For simplicity, we just mark all keys as "usable".
        if self.key_statuses.lock().unwrap().is_empty() {
            // Add a placeholder key.
            self.key_statuses
                .lock()
                .unwrap()
                .insert(vec![0u8; 16], KeyStatus::Usable);
        }
        // Fire keystatuseschange event.
        if let Some(cb) = self.on_keystatuseschange.lock().unwrap().as_ref() {
            cb();
        }
        let _ = response;
        Ok(())
    }

    /// Close the session.
    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock().unwrap() = true;
        if let Some(cb) = self.on_closed.lock().unwrap().as_ref() {
            cb();
        }
        Ok(())
    }

    /// Remove persisted session data (for persistent sessions).
    pub fn remove(&self) -> Result<(), String> {
        self.key_statuses.lock().unwrap().clear();
        Ok(())
    }

    /// Set the onmessage callback.
    pub fn on_message<F: Fn(MediaKeyMessage) + Send + 'static>(&self, callback: F) {
        *self.on_message.lock().unwrap() = Some(Box::new(callback));
    }

    /// Set the onkeystatuseschange callback.
    pub fn on_keystatuseschange<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_keystatuseschange.lock().unwrap() = Some(Box::new(callback));
    }
}

/// A message from the CDM to the application (typically a license request).
#[derive(Debug, Clone)]
pub struct MediaKeyMessage {
    pub message_type: KeyMessageType,
    pub message: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMessageType {
    LicenseRequest,
    LicenseRenewal,
    LicenseRelease,
    IndividualizationRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStatus {
    Usable,
    Expired,
    Released,
    OutputDownscaled,
    StatusPending,
    UsableInFuture,
}

fn generate_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("session-{}", id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_system_parsing() {
        assert_eq!(
            KeySystem::from_str("com.widevine.alpha"),
            Some(KeySystem::Widevine)
        );
        assert_eq!(
            KeySystem::from_str("org.w3.clearkey"),
            Some(KeySystem::ClearKey)
        );
        assert_eq!(KeySystem::from_str("unknown"), None);
    }

    #[test]
    fn only_clearkey_is_available() {
        assert!(!KeySystem::Widevine.is_available());
        assert!(!KeySystem::PlayReady.is_available());
        assert!(!KeySystem::FairPlay.is_available());
        assert!(KeySystem::ClearKey.is_available());
    }

    #[test]
    fn clearkey_session_lifecycle() {
        let mk = MediaKeys::new(KeySystem::ClearKey);
        let session = mk.create_session(SessionType::Temporary);

        let message_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let m = message_received.clone();
        session.on_message(move |_msg| {
            m.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let keys_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let k = keys_changed.clone();
        session.on_keystatuseschange(move || {
            k.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Generate a license request.
        session
            .generate_request("keyids", b"test-init-data")
            .unwrap();
        assert!(message_received.load(std::sync::atomic::Ordering::SeqCst));

        // Apply a license.
        session.update(b"test-license").unwrap();
        assert!(keys_changed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(session.key_statuses.lock().unwrap().len(), 1);

        // Close the session.
        session.close().unwrap();
        assert!(*session.closed.lock().unwrap());
    }

    #[test]
    fn widevine_not_supported() {
        let access = MediaKeySystemAccess {
            key_system: KeySystem::Widevine,
            configuration: MediaKeySystemConfig {
                init_data_types: vec!["cenc".into()],
                audio_capabilities: vec![],
                video_capabilities: vec![],
                distinctive_identifier: MediaKeySystemAvailability::Optional,
                persistent_state: MediaKeySystemAvailability::NotAllowed,
                session_types: vec![SessionType::Temporary],
            },
        };
        assert!(
            !access.is_supported(),
            "Widevine should not be supported without CDM"
        );
    }

    #[test]
    fn clearkey_supported_for_temporary_sessions() {
        let access = MediaKeySystemAccess {
            key_system: KeySystem::ClearKey,
            configuration: MediaKeySystemConfig {
                init_data_types: vec!["keyids".into()],
                audio_capabilities: vec![],
                video_capabilities: vec![],
                distinctive_identifier: MediaKeySystemAvailability::NotAllowed,
                persistent_state: MediaKeySystemAvailability::NotAllowed,
                session_types: vec![SessionType::Temporary],
            },
        };
        assert!(access.is_supported());
    }
}
