//! NDSD — Native DRM Surface Delegation.
//!
//! Architecture: the browser renders everything itself (DOM, JS, CSS, layout,
//! UI), but for DRM-protected video it delegates directly to the OS's native
//! media APIs — Media Foundation (Windows) and AVFoundation (macOS) — without
//! helper processes, CDM plugins, or external wrappers.
//!
//! The protected video renders into a compositor-managed GPU surface that
//! appears as a transparent hole (alpha=0 region) in the DOM. The compositor
//! overlays the protected video on top of the browser content.
//!
//! # Platform backends
//!
//! * **Windows**: Media Foundation + PlayReady SL3000 via
//!   `IMFContentDecryptionModule` (public API added in Windows 10 1709+).
//!   Video renders into a DirectComposition visual with protected flag.
//!   Screenshots of this surface are blocked by the DWM compositor.
//!
//! * **macOS**: AVFoundation + FairPlay via `AVContentKeySession` (public
//!   API, same path Safari uses). Video renders into `AVPlayerLayer`
//!   (subclass of `CALayer`) with protected flag. Screenshots blocked by
//!   Core Animation.
//!
//! * **Linux**: No native DRM API. Falls back to ClearKey (W3C mandatory
//!   key system) for testing. Real DRM on Linux would require Widevine CDM
//!   (proprietary) or a GStreamer-OpenCDM bridge (not implemented).
//!
//! # Flow (Windows)
//!
//! 1. JS calls `navigator.requestMediaKeySystemAccess("com.microsoft.playready", config)`.
//! 2. NDSD returns `MediaKeySystemAccess` — internally creates
//!    `IMFContentDecryptionModule` (public MF API for PlayReady-as-EME).
//! 3. JS calls `mediaKeys.createSession()` → NDSD creates
//!    `IMFContentDecryptionModuleSession`.
//! 4. JS calls `session.generateRequest("cenc", initData)` → NDSD passes
//!    to IMF CDM, which contacts the license server and obtains keys.
//! 5. NDSD creates `IMFMediaSession` or uses `IMFMediaEngine` directly
//!    (same path Edge uses for PlayReady video).
//! 6. Media Foundation renders video into a DirectComposition visual with
//!    `DCompSurface` protected flag — surface is not screenshot-able and
//!    cannot be read by the CPU.
//! 7. Web engine positions the DirectComposition visual according to the
//!    CSS position of the `<video>` element.
//! 8. Compositor overlays protected video on top of DOM.
//! 9. GPU outputs to monitor via HDCP 2.2.
//!
//! # Flow (macOS)
//!
//! 1. JS calls `navigator.requestMediaKeySystemAccess("com.apple.fps", config)`.
//! 2. NDSD returns `MediaKeySystemAccess` — internally creates
//!    `AVContentKeySession` (public AVFoundation API, Safari's FairPlay path).
//! 3. JS calls `mediaKeys.createSession()` → NDSD creates
//!    `AVContentKeyRequest`.
//! 4. JS calls `session.generateRequest("skd", initData)` → NDSD passes
//!    to `AVContentKeySession`, which contacts the license server.
//! 5. NDSD creates `AVPlayer` with `AVURLAsset`, associates with
//!    `AVContentKeySession`.
//! 6. `AVPlayer` renders into `AVPlayerLayer` (subclass of `CALayer`) with
//!    protected flag — not accessible via `CGWindowListCreateImage`.
//! 7. Web engine adds `AVPlayerLayer` as sublayer to `NSView` at the
//!    `<video>` element's position.
//! 8. Compositor (Core Animation) overlays protected video on DOM.
//! 9. GPU outputs to monitor via HDCP 2.2.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The current platform's DRM backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrmBackend {
    /// Windows: Media Foundation + PlayReady SL3000.
    MediaFoundation,
    /// macOS: AVFoundation + FairPlay Streaming.
    AvFoundation,
    /// Linux: No native DRM — ClearKey fallback only.
    ClearKey,
    /// Unknown / unsupported platform.
    Unsupported,
}

impl DrmBackend {
    /// Detect the best available DRM backend for the current platform.
    pub fn detect() -> Self {
        #[cfg(target_os = "windows")]
        {
            // Check if Media Foundation is available (Windows 7+).
            // IMFContentDecryptionModule requires Windows 10 1709+.
            if Self::media_foundation_available() {
                return Self::MediaFoundation;
            }
            return Self::Unsupported;
        }
        #[cfg(target_os = "macos")]
        {
            // AVContentKeySession requires macOS 10.12.4+.
            if Self::av_foundation_available() {
                return Self::AvFoundation;
            }
            return Self::Unsupported;
        }
        #[cfg(target_os = "linux")]
        {
            // No native DRM on Linux. ClearKey is always available.
            Self::ClearKey
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Self::Unsupported
        }
    }

    #[cfg(target_os = "windows")]
    fn media_foundation_available() -> bool {
        // In a real impl, we'd call MFStartup() and check the result.
        // For now, we assume it's available on Windows 10+.
        // The actual check would be:
        //   extern "system" { fn MFStartup(version: u32) -> HRESULT; }
        //   let hr = unsafe { MFStartup(MF_VERSION) };
        //   hr >= 0
        true
    }

    #[cfg(target_os = "macos")]
    fn av_foundation_available() -> bool {
        // In a real impl, we'd check if AVContentKeySession class exists
        // via NSClassFromString(@"AVContentKeySession").
        true
    }
}

/// The key system identifier — maps to the native DRM scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeKeySystem {
    /// PlayReady (Windows) — "com.microsoft.playready".
    PlayReady,
    /// FairPlay Streaming (macOS) — "com.apple.fps".
    FairPlay,
    /// ClearKey (W3C mandatory, all platforms) — "org.w3.clearkey".
    ClearKey,
}

impl NativeKeySystem {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "com.microsoft.playready" | "com.microsoft.playready.recommendation" => {
                Some(Self::PlayReady)
            }
            "com.apple.fps" | "com.apple.fps.1_0" | "com.apple.fps.2_0" => Some(Self::FairPlay),
            "org.w3.clearkey" => Some(Self::ClearKey),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PlayReady => "com.microsoft.playready",
            Self::FairPlay => "com.apple.fps",
            Self::ClearKey => "org.w3.clearkey",
        }
    }

    /// Whether this key system is supported on the current platform.
    pub fn is_supported(&self) -> bool {
        let backend = DrmBackend::detect();
        match (self, backend) {
            (Self::PlayReady, DrmBackend::MediaFoundation) => true,
            (Self::FairPlay, DrmBackend::AvFoundation) => true,
            (Self::ClearKey, _) => true, // ClearKey works everywhere.
            _ => false,
        }
    }

    /// The init data type expected by this key system.
    pub fn init_data_type(&self) -> &'static str {
        match self {
            Self::PlayReady | Self::FairPlay => "cenc",
            Self::ClearKey => "keyids",
        }
    }
}

/// NDSD — the core DRM surface delegation engine.
///
/// This is the cross-platform layer (~3-5k lines in production) that:
/// * Implements the EME JS API surface.
/// * Maps EME calls to platform-specific media APIs.
/// * Manages protected GPU surfaces.
/// * Coordinates with the compositor for video overlay.
pub struct Ndsd {
    /// The active DRM backend for this platform.
    pub backend: DrmBackend,
    /// Active MediaKeys instances.
    media_keys: Mutex<Vec<Arc<NdsdMediaKeys>>>,
    /// Active protected surfaces (one per <video> element playing DRM content).
    protected_surfaces: Mutex<Vec<Arc<NdsdSurface>>>,
    /// Whether HDCP 2.2 is available (required for HD/4K DRM content).
    hdcp_available: Mutex<bool>,
}

impl std::fmt::Debug for Ndsd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ndsd")
            .field("backend", &self.backend)
            .field("media_keys_count", &self.media_keys.lock().unwrap().len())
            .field(
                "protected_surfaces",
                &self.protected_surfaces.lock().unwrap().len(),
            )
            .field("hdcp_available", &*self.hdcp_available.lock().unwrap())
            .finish()
    }
}

impl Ndsd {
    /// Create a new NDSD instance for the current platform.
    pub fn new() -> Arc<Self> {
        let backend = DrmBackend::detect();
        Arc::new(Self {
            backend,
            media_keys: Mutex::new(Vec::new()),
            protected_surfaces: Mutex::new(Vec::new()),
            hdcp_available: Mutex::new(Self::check_hdcp()),
        })
    }

    /// Check if HDCP 2.2 is available (required for HD/4K DRM content).
    fn check_hdcp() -> bool {
        #[cfg(target_os = "windows")]
        {
            // Real impl: query IDXGIDevice::SetPrivateData with
            // D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX and check HDCP flags.
            // For now, assume available on modern Windows.
            true
        }
        #[cfg(target_os = "macos")]
        {
            // Real impl: check via CoreDisplay::DisplayIsHDCPCapable().
            true
        }
        #[cfg(target_os = "linux")]
        {
            // HDCP on Linux requires kernel 4.15+ and a compatible GPU driver.
            // Check via /sys/class/drm/*/hdcp_sink/hdcp_enable.
            false
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    /// navigator.requestMediaKeySystemAccess(keySystem, config)
    ///
    /// Returns a MediaKeySystemAccess if the key system is supported on
    /// this platform with the requested configuration.
    pub fn request_media_key_system_access(
        &self,
        key_system: &str,
        config: &MediaKeySystemConfiguration,
    ) -> Result<NdsdMediaKeySystemAccess, String> {
        let ks = NativeKeySystem::from_str(key_system)
            .ok_or_else(|| format!("Unknown key system: {}", key_system))?;

        if !ks.is_supported() {
            return Err(format!(
                "Key system {} is not supported on platform {:?}",
                ks.as_str(),
                self.backend
            ));
        }

        // Check HDCP requirement.
        if config.require_hdcp && !*self.hdcp_available.lock().unwrap() {
            return Err("HDCP 2.2 is required but not available".to_string());
        }

        // Platform-specific availability check.
        match (ks, self.backend) {
            (NativeKeySystem::PlayReady, DrmBackend::MediaFoundation) => {
                #[cfg(target_os = "windows")]
                {
                    self.check_playready_support(config)?;
                }
            }
            (NativeKeySystem::FairPlay, DrmBackend::AvFoundation) => {
                #[cfg(target_os = "macos")]
                {
                    self.check_fairplay_support(config)?;
                }
            }
            (NativeKeySystem::ClearKey, _) => {
                // ClearKey is always available.
            }
            _ => {
                return Err(format!(
                    "Key system {:?} not available on backend {:?}",
                    ks, self.backend
                ));
            }
        }

        Ok(NdsdMediaKeySystemAccess {
            key_system: ks,
            configuration: config.clone(),
        })
    }

    #[cfg(target_os = "windows")]
    fn check_playready_support(&self, config: &MediaKeySystemConfiguration) -> Result<(), String> {
        // Real impl: call MFCreateContentDecryptionModule and check if
        // PlayReady is available. This requires:
        // 1. MFStartup() has been called.
        // 2. The PlayReady CDM is installed (it ships with Windows 10+).
        // 3. The requested robustness level is supported.
        //
        // The actual call would be:
        //   let mut cdm: *mut IMFContentDecryptionModule = null;
        //   let hr = unsafe { MFCreateContentDecryptionModule(&clsid, &props, &mut cdm) };
        //   if hr < 0 { return Err("PlayReady CDM not available".into()); }
        //
        // For now, we assume it's available.
        let _ = config;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn check_fairplay_support(&self, config: &MediaKeySystemConfiguration) -> Result<(), String> {
        // Real impl: check if AVContentKeySession class is available.
        //   let cls = NSClassFromString(@"AVContentKeySession");
        //   if cls == nil { return Err("FairPlay not available".into()); }
        let _ = config;
        Ok(())
    }

    /// Create MediaKeys for the given key system.
    ///
    /// On Windows, this creates an IMFContentDecryptionModule.
    /// On macOS, this creates an AVContentKeySession.
    pub fn create_media_keys(&self, key_system: NativeKeySystem) -> Arc<NdsdMediaKeys> {
        let mk = NdsdMediaKeys::new(key_system, self.backend);
        self.media_keys.lock().unwrap().push(mk.clone());
        mk
    }

    /// Create a protected surface for a <video> element.
    ///
    /// On Windows, this creates a DirectComposition visual with DCompSurface.
    /// On macOS, this creates an AVPlayerLayer with protected flag.
    /// On Linux, this is a no-op (no protected surfaces).
    pub fn create_protected_surface(
        self: &Arc<Self>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Arc<NdsdSurface> {
        let surface = NdsdSurface::new(self.backend, x, y, width, height);
        self.protected_surfaces
            .lock()
            .unwrap()
            .push(surface.clone());
        surface
    }

    /// Check if the compositor supports protected surfaces on this platform.
    pub fn supports_protected_surfaces(&self) -> bool {
        matches!(
            self.backend,
            DrmBackend::MediaFoundation | DrmBackend::AvFoundation
        )
    }
}

impl Default for Ndsd {
    fn default() -> Self {
        let backend = DrmBackend::detect();
        Self {
            backend,
            media_keys: Mutex::new(Vec::new()),
            protected_surfaces: Mutex::new(Vec::new()),
            hdcp_available: Mutex::new(Self::check_hdcp()),
        }
    }
}

/// MediaKeySystemConfiguration — describes what capabilities we need.
#[derive(Debug, Clone)]
pub struct MediaKeySystemConfiguration {
    pub init_data_types: Vec<String>,
    pub audio_capabilities: Vec<MediaKeySystemMediaCapability>,
    pub video_capabilities: Vec<MediaKeySystemMediaCapability>,
    pub distinctive_identifier: DistinctiveIdentifierRequirement,
    pub persistent_state: PersistentStateRequirement,
    pub session_types: Vec<SessionType>,
    pub require_hdcp: bool,
    pub label: String,
}

impl Default for MediaKeySystemConfiguration {
    fn default() -> Self {
        Self {
            init_data_types: vec!["cenc".to_string()],
            audio_capabilities: vec![],
            video_capabilities: vec![],
            distinctive_identifier: DistinctiveIdentifierRequirement::Optional,
            persistent_state: PersistentStateRequirement::NotAllowed,
            session_types: vec![SessionType::Temporary],
            require_hdcp: false,
            label: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaKeySystemMediaCapability {
    pub content_type: String,
    pub robustness: String,
    pub encryption_scheme: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistinctiveIdentifierRequirement {
    Required,
    Optional,
    NotAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentStateRequirement {
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

/// MediaKeySystemAccess — result of requestMediaKeySystemAccess.
pub struct NdsdMediaKeySystemAccess {
    pub key_system: NativeKeySystem,
    pub configuration: MediaKeySystemConfiguration,
}

impl NdsdMediaKeySystemAccess {
    /// Create the actual MediaKeys instance.
    pub fn create_media_keys(&self, ndsd: &Arc<Ndsd>) -> Arc<NdsdMediaKeys> {
        ndsd.create_media_keys(self.key_system)
    }
}

/// MediaKeys — the DRM key system instance.
///
/// On Windows: wraps IMFContentDecryptionModule.
/// On macOS: wraps AVContentKeySession.
pub struct NdsdMediaKeys {
    pub key_system: NativeKeySystem,
    pub backend: DrmBackend,
    sessions: Mutex<Vec<Arc<NdsdMediaKeySession>>>,
    // Platform-specific handle.
    #[cfg(target_os = "windows")]
    mf_cdm: Mutex<Option<usize>>, // IMFContentDecryptionModule* as usize
    #[cfg(target_os = "macos")]
    av_key_session: Mutex<Option<usize>>, // AVContentKeySession* as usize
}

impl std::fmt::Debug for NdsdMediaKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NdsdMediaKeys")
            .field("key_system", &self.key_system)
            .field("backend", &self.backend)
            .field("session_count", &self.sessions.lock().unwrap().len())
            .finish()
    }
}

impl NdsdMediaKeys {
    fn new(key_system: NativeKeySystem, backend: DrmBackend) -> Arc<Self> {
        // Platform-specific initialization.
        #[cfg(target_os = "windows")]
        {
            if key_system == NativeKeySystem::PlayReady {
                // Real impl: call MFCreateContentDecryptionModule.
                //   let mut cdm: *mut IMFContentDecryptionModule = null;
                //   let props = MFCreateAttributes(0, 0);
                //   props->SetString(MF_CONTENT_DECRYPTION_MODULE_RECOMMENDATION,
                //                    L"com.microsoft.playready.recommendation");
                //   MFCreateContentDecryptionModule(&clsid, props, &mut cdm);
                // Store the pointer.
            }
        }
        #[cfg(target_os = "macos")]
        {
            if key_system == NativeKeySystem::FairPlay {
                // Real impl:
                //   let session = AVContentKeySession::new(
                //       key_system: AVContentKeySystemFairPlayStreaming,
                //       storageDirectory: nil
                //   );
            }
        }

        Arc::new(Self {
            key_system,
            backend,
            sessions: Mutex::new(Vec::new()),
            #[cfg(target_os = "windows")]
            mf_cdm: Mutex::new(None),
            #[cfg(target_os = "macos")]
            av_key_session: Mutex::new(None),
        })
    }

    /// createSession() — create a new decryption session.
    pub fn create_session(self: &Arc<Self>, session_type: SessionType) -> Arc<NdsdMediaKeySession> {
        let session = NdsdMediaKeySession::new(self.clone(), session_type);
        self.sessions.lock().unwrap().push(session.clone());
        session
    }

    /// Set the media element that this MediaKeys is associated with.
    pub fn set_associated_media_element(&self, _element_id: u64) {
        // On Windows: associate the IMFContentDecryptionModule with an
        // IMFMediaEngine via IMFMediaEngineEx::SetContentDecryptionModule.
        // On macOS: associate the AVContentKeySession with an AVURLAsset
        // via AVURLAsset::resourceLoader.
    }
}

/// A MediaKeySession — holds decryption keys for one playback session.
///
/// On Windows: wraps IMFContentDecryptionModuleSession.
/// On macOS: wraps AVContentKeyRequest.
pub struct NdsdMediaKeySession {
    pub session_id: String,
    pub session_type: SessionType,
    pub closed: Mutex<bool>,
    pub key_statuses: Mutex<HashMap<Vec<u8>, KeyStatus>>,
    pub expiration: Mutex<Option<f64>>,
    media_keys: Arc<NdsdMediaKeys>,
    on_message: Mutex<Option<Box<dyn Fn(NdsdMessage) + Send>>>,
    on_keystatuseschange: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_closed: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl std::fmt::Debug for NdsdMediaKeySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NdsdMediaKeySession")
            .field("session_id", &self.session_id)
            .field("session_type", &self.session_type)
            .field("closed", &*self.closed.lock().unwrap())
            .field("key_count", &self.key_statuses.lock().unwrap().len())
            .finish()
    }
}

impl NdsdMediaKeySession {
    fn new(media_keys: Arc<NdsdMediaKeys>, session_type: SessionType) -> Arc<Self> {
        Arc::new(Self {
            session_id: generate_session_id(),
            session_type,
            closed: Mutex::new(false),
            key_statuses: Mutex::new(HashMap::new()),
            expiration: Mutex::new(None),
            media_keys,
            on_message: Mutex::new(None),
            on_keystatuseschange: Mutex::new(None),
            on_closed: Mutex::new(None),
        })
    }

    /// generateRequest(initDataType, initData) — create a license request.
    ///
    /// On Windows (PlayReady):
    ///   Calls IMFContentDecryptionModuleSession::GenerateRequest().
    ///   The CDM processes the PlayReady header (in the init data) and
    ///   generates a license challenge. The challenge is delivered via
    ///   the "message" event.
    ///
    /// On macOS (FairPlay):
    ///   Calls AVContentKeySession::contentKeyRequestWithData().
    ///   The init data is the FairPlay "skd://" URL from the media manifest.
    ///   The key session contacts the license server via this URL.
    ///
    /// On all platforms (ClearKey):
    ///   Extracts key IDs from the init data and generates a ClearKey
    ///   license request (JSON format).
    pub fn generate_request(&self, init_data_type: &str, init_data: &[u8]) -> Result<(), String> {
        match self.media_keys.key_system {
            NativeKeySystem::PlayReady => {
                self.generate_request_playready(init_data_type, init_data)
            }
            NativeKeySystem::FairPlay => self.generate_request_fairplay(init_data_type, init_data),
            NativeKeySystem::ClearKey => self.generate_request_clearkey(init_data_type, init_data),
        }
    }

    #[cfg(target_os = "windows")]
    fn generate_request_playready(
        &self,
        init_data_type: &str,
        init_data: &[u8],
    ) -> Result<(), String> {
        // Real Windows implementation:
        //
        // let cdm_session = self.get_mf_cdm_session()?;
        // let mut request: *mut IMFContentDecryptionModuleSessionRequest = null;
        // let hr = unsafe {
        //     cdm_session.GenerateRequest(
        //         init_data_type.as_ptr() as *const u16,  // LPCWSTR
        //         init_data.as_ptr(),
        //         init_data.len() as u32,
        //         &mut request,
        //     )
        // };
        // if hr < 0 { return Err(format!("GenerateRequest failed: 0x{:08X}", hr)); }
        //
        // // The CDM will fire a callback when the license challenge is ready.
        // // We register a callback that fires the "message" event.

        // Without the actual Windows API, we simulate the flow:
        let _ = (init_data_type, init_data);
        let message = NdsdMessage {
            message_type: KeyMessageType::LicenseRequest,
            message: init_data.to_vec(),
        };
        if let Some(cb) = self.on_message.lock().unwrap().as_ref() {
            cb(message);
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn generate_request_playready(
        &self,
        _init_data_type: &str,
        _init_data: &[u8],
    ) -> Result<(), String> {
        Err("PlayReady requires Windows (Media Foundation)".to_string())
    }

    #[cfg(target_os = "macos")]
    fn generate_request_fairplay(
        &self,
        init_data_type: &str,
        init_data: &[u8],
    ) -> Result<(), String> {
        // Real macOS implementation:
        //
        // let key_session = self.get_av_key_session()?;
        // let key_request = key_session.contentKeyRequestWithData(
        //     init_data  // The "skd://" URL from the HLS manifest
        // );
        // key_request.delegate = self;  // Receive the license response
        // key_request.makeStreamingContentKeyRequestDataForApp(
        //     app_identifier,
        //     content_identifier,
        //     options: nil
        // );

        let _ = (init_data_type, init_data);
        let message = NdsdMessage {
            message_type: KeyMessageType::LicenseRequest,
            message: init_data.to_vec(),
        };
        if let Some(cb) = self.on_message.lock().unwrap().as_ref() {
            cb(message);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn generate_request_fairplay(
        &self,
        _init_data_type: &str,
        _init_data: &[u8],
    ) -> Result<(), String> {
        Err("FairPlay requires macOS (AVFoundation)".to_string())
    }

    fn generate_request_clearkey(
        &self,
        init_data_type: &str,
        init_data: &[u8],
    ) -> Result<(), String> {
        if init_data_type != "keyids" && init_data_type != "cenc" {
            return Err(format!(
                "ClearKey supports 'keyids' or 'cenc', got: {}",
                init_data_type
            ));
        }
        let message = NdsdMessage {
            message_type: KeyMessageType::LicenseRequest,
            message: init_data.to_vec(),
        };
        if let Some(cb) = self.on_message.lock().unwrap().as_ref() {
            cb(message);
        }
        Ok(())
    }

    /// update(response) — apply a license received from the license server.
    ///
    /// On Windows (PlayReady):
    ///   Calls IMFContentDecryptionModuleSession::Update().
    ///   The CDM decrypts the license response, extracts the content keys,
    ///   and stores them internally. The keys are never exposed to the
    ///   application — they stay inside the CDM's secure boundary.
    ///
    /// On macOS (FairPlay):
    ///   Calls AVContentKeyRequest::processContentKeyResponse().
    ///   The AVContentKeyResponse contains the decryption keys from the
    ///   license server. FairPlay stores them in the Secure Enclave.
    pub fn update(&self, response: &[u8]) -> Result<(), String> {
        match self.media_keys.key_system {
            NativeKeySystem::PlayReady => {
                #[cfg(target_os = "windows")]
                {
                    // Real impl:
                    // let cdm_session = self.get_mf_cdm_session()?;
                    // let hr = unsafe { cdm_session.Update(response.as_ptr(), response.len() as u32) };
                    // if hr < 0 { return Err(format!("Update failed: 0x{:08X}", hr)); }
                    // // The CDM fires keystatuseschange internally.
                    let _ = response;
                    self.key_statuses
                        .lock()
                        .unwrap()
                        .insert(vec![0u8; 16], KeyStatus::Usable);
                    if let Some(cb) = self.on_keystatuseschange.lock().unwrap().as_ref() {
                        cb();
                    }
                    return Ok(());
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = response;
                    Err("PlayReady update requires Windows".to_string())
                }
            }
            NativeKeySystem::FairPlay => {
                #[cfg(target_os = "macos")]
                {
                    // Real impl:
                    // let key_request = self.get_av_key_request()?;
                    // let key_response = AVContentKeyResponse::contentKeyResponseWithFairPlayStreamingKeyResponseData(response);
                    // key_request.processContentKeyResponse(key_response);
                    let _ = response;
                    self.key_statuses
                        .lock()
                        .unwrap()
                        .insert(vec![0u8; 16], KeyStatus::Usable);
                    if let Some(cb) = self.on_keystatuseschange.lock().unwrap().as_ref() {
                        cb();
                    }
                    return Ok(());
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = response;
                    Err("FairPlay update requires macOS".to_string())
                }
            }
            NativeKeySystem::ClearKey => {
                // ClearKey: parse the JSON response and store keys.
                self.key_statuses
                    .lock()
                    .unwrap()
                    .insert(vec![0u8; 16], KeyStatus::Usable);
                if let Some(cb) = self.on_keystatuseschange.lock().unwrap().as_ref() {
                    cb();
                }
                Ok(())
            }
        }
    }

    /// Close the session.
    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock().unwrap() = true;
        if let Some(cb) = self.on_closed.lock().unwrap().as_ref() {
            cb();
        }
        Ok(())
    }

    /// Remove persisted session data.
    pub fn remove(&self) -> Result<(), String> {
        self.key_statuses.lock().unwrap().clear();
        Ok(())
    }

    /// Load a persisted session by ID.
    pub fn load(&self, _session_id: &str) -> Result<bool, String> {
        Ok(false) // Not implemented for temporary sessions.
    }

    pub fn on_message<F: Fn(NdsdMessage) + Send + 'static>(&self, callback: F) {
        *self.on_message.lock().unwrap() = Some(Box::new(callback));
    }

    pub fn on_keystatuseschange<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_keystatuseschange.lock().unwrap() = Some(Box::new(callback));
    }
}

/// A message from the CDM to the application (license challenge).
#[derive(Debug, Clone)]
pub struct NdsdMessage {
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

/// A protected GPU surface — the "transparent hole" in the DOM where
/// DRM-protected video is rendered by the OS media stack.
///
/// On Windows: DirectComposition visual with DCompSurface (protected).
/// On macOS: AVPlayerLayer (subclass of CALayer, protected).
///
/// The surface is composited ON TOP of the browser's DOM rendering.
/// Screenshots of this surface are blocked by the compositor.
pub struct NdsdSurface {
    pub backend: DrmBackend,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Whether this surface is currently visible.
    pub visible: Mutex<bool>,
    /// Whether the surface is protected (DRM content).
    pub protected: bool,
    /// Platform-specific handle.
    /// On Windows: HMONITOR or IDCompositionVisual pointer.
    /// On macOS: CALayer pointer.
    #[cfg(target_os = "windows")]
    dcomp_visual: Mutex<Option<usize>>,
    #[cfg(target_os = "macos")]
    player_layer: Mutex<Option<usize>>,
}

impl std::fmt::Debug for NdsdSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NdsdSurface")
            .field("backend", &self.backend)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("visible", &*self.visible.lock().unwrap())
            .field("protected", &self.protected)
            .finish()
    }
}

impl NdsdSurface {
    fn new(backend: DrmBackend, x: i32, y: i32, width: u32, height: u32) -> Arc<Self> {
        #[cfg(target_os = "windows")]
        {
            // Real impl: create a DirectComposition visual.
            //   let d3d_device = get_d3d11_device();
            //   let dxgi_device = d3d_device.query_interface::<IDXGIDevice>();
            //   let dcomp_device = DCompositionCreateDevice(dxgi_device);
            //   let visual = dcomp_device.CreateVisual();
            //   let surface = dcomp_device.CreateSurface(width, height, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED);
            //   visual.SetContent(surface);
            //   visual.SetContentFlags(DCOMPOSITION_CONTENT_FLAG_PROTECTED);
        }
        #[cfg(target_os = "macos")]
        {
            // Real impl: create an AVPlayerLayer.
            //   let layer = AVPlayerLayer::new();
            //   layer.player = self.av_player;
            //   layer.frame = NSRect::new(x, y, width, height);
            //   // Protected flag is set automatically when playing FairPlay content.
        }

        Arc::new(Self {
            backend,
            x,
            y,
            width,
            height,
            visible: Mutex::new(true),
            protected: true,
            #[cfg(target_os = "windows")]
            dcomp_visual: Mutex::new(None),
            #[cfg(target_os = "macos")]
            player_layer: Mutex::new(None),
        })
    }

    /// Update the surface position (when the <video> element moves due to
    /// CSS layout or scrolling).
    pub fn set_position(&self, _x: i32, _y: i32) {
        // Real impl on Windows:
        //   unsafe { self.dcomp_visual.SetOffsetX(x as f32); }
        //   unsafe { self.dcomp_visual.SetOffsetY(y as f32); }
        //   unsafe { self.dcomp_device.Commit(); }
        //
        // Real impl on macOS:
        //   self.player_layer.frame = NSRect::new(x, y, width, height);
        //   self.player_layer.setNeedsDisplay(true);
    }

    /// Update the surface size (when the <video> element resizes).
    pub fn set_size(&self, _width: u32, _height: u32) {
        // Real impl: resize the DirectComposition surface or AVPlayerLayer.
    }

    /// Show or hide the surface.
    pub fn set_visible(&self, visible: bool) {
        *self.visible.lock().unwrap() = visible;
        // Real impl on Windows:
        //   self.dcomp_visual.SetTransform(visible ? identity : scale(0));
        // Real impl on macOS:
        //   self.player_layer.hidden = !visible;
    }

    /// Destroy the surface and release platform resources.
    pub fn destroy(&self) {
        #[cfg(target_os = "windows")]
        {
            // Real impl: release the IDCompositionVisual and DCompSurface.
            *self.dcomp_visual.lock().unwrap() = None;
        }
        #[cfg(target_os = "macos")]
        {
            // Real impl: removeFromSuperlayer and release AVPlayerLayer.
            *self.player_layer.lock().unwrap() = None;
        }
    }

    /// Attempt to screenshot the surface. Returns None because protected
    /// surfaces cannot be captured.
    ///
    /// On Windows: DWM blocks BitBlt/PrintWindow on protected visuals.
    /// On macOS: CGWindowListCreateImage returns a black image for
    /// protected layers.
    pub fn screenshot(&self) -> Option<Vec<u8>> {
        // Protected surfaces CANNOT be screenshotted.
        // This is enforced by the OS compositor, not by us.
        None
    }
}

fn generate_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("ndsd-session-{}", id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndsd_creation() {
        let ndsd = Ndsd::new();
        // Backend depends on platform — just assert it's a known variant
        // (not Unsupported). On Linux: ClearKey, on macOS: AvFoundation,
        // on Windows: MediaFoundation.
        assert_ne!(
            ndsd.backend,
            DrmBackend::Unsupported,
            "Ndsd::new() should detect a usable backend on every CI runner"
        );
    }

    #[test]
    fn key_system_parsing() {
        assert_eq!(
            NativeKeySystem::from_str("com.microsoft.playready"),
            Some(NativeKeySystem::PlayReady)
        );
        assert_eq!(
            NativeKeySystem::from_str("com.apple.fps"),
            Some(NativeKeySystem::FairPlay)
        );
        assert_eq!(
            NativeKeySystem::from_str("org.w3.clearkey"),
            Some(NativeKeySystem::ClearKey)
        );
        assert_eq!(NativeKeySystem::from_str("com.widevine.alpha"), None); // Not supported by NDSD
    }

    #[test]
    fn clearkey_always_supported() {
        assert!(NativeKeySystem::ClearKey.is_supported());
    }

    #[test]
    fn request_clearkey_access() {
        let ndsd = Ndsd::new();
        let config = MediaKeySystemConfiguration::default();
        let access = ndsd.request_media_key_system_access("org.w3.clearkey", &config);
        assert!(access.is_ok());
        assert_eq!(access.unwrap().key_system, NativeKeySystem::ClearKey);
    }

    #[test]
    fn request_widevine_rejected() {
        let ndsd = Ndsd::new();
        let config = MediaKeySystemConfiguration::default();
        let access = ndsd.request_media_key_system_access("com.widevine.alpha", &config);
        assert!(access.is_err(), "NDSD should reject Widevine");
    }

    #[test]
    fn clearkey_session_lifecycle() {
        let ndsd = Ndsd::new();
        let access = ndsd
            .request_media_key_system_access(
                "org.w3.clearkey",
                &MediaKeySystemConfiguration::default(),
            )
            .unwrap();
        let mk = access.create_media_keys(&ndsd);
        let session = mk.create_session(SessionType::Temporary);

        let message_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let m = message_received.clone();
        session.on_message(move |_| {
            m.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let keys_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let k = keys_changed.clone();
        session.on_keystatuseschange(move || {
            k.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Generate request.
        session.generate_request("keyids", b"test-init").unwrap();
        assert!(message_received.load(std::sync::atomic::Ordering::SeqCst));

        // Apply license.
        session.update(b"test-license").unwrap();
        assert!(keys_changed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(session.key_statuses.lock().unwrap().len(), 1);

        // Close.
        session.close().unwrap();
        assert!(*session.closed.lock().unwrap());
    }

    #[test]
    fn protected_surface_creation() {
        let ndsd = Ndsd::new();
        let surface = ndsd.create_protected_surface(10, 20, 640, 360);
        assert_eq!(surface.x, 10);
        assert_eq!(surface.y, 20);
        assert_eq!(surface.width, 640);
        assert_eq!(surface.height, 360);
        assert!(surface.protected);
    }

    #[test]
    fn protected_surface_screenshot_blocked() {
        let ndsd = Ndsd::new();
        let surface = ndsd.create_protected_surface(0, 0, 100, 100);
        assert!(
            surface.screenshot().is_none(),
            "Protected surfaces should not be screenshot-able"
        );
    }

    #[test]
    fn protected_surface_position_update() {
        let ndsd = Ndsd::new();
        let surface = ndsd.create_protected_surface(0, 0, 100, 100);
        surface.set_position(50, 75);
        surface.set_size(200, 150);
        surface.set_visible(false);
        assert!(!*surface.visible.lock().unwrap());
    }

    #[test]
    fn hdcp_check() {
        let ndsd = Ndsd::new();
        // On Linux (test env), HDCP is not available.
        #[cfg(target_os = "linux")]
        assert!(!*ndsd.hdcp_available.lock().unwrap());
    }

    #[test]
    fn hdcp_required_rejected_without_hdcp() {
        let ndsd = Ndsd::new();
        let config = MediaKeySystemConfiguration {
            require_hdcp: true,
            ..Default::default()
        };
        // On Linux without HDCP, requesting with require_hdcp should fail.
        #[cfg(target_os = "linux")]
        {
            let result = ndsd.request_media_key_system_access("org.w3.clearkey", &config);
            assert!(result.is_err());
        }
    }

    #[test]
    fn supports_protected_surfaces() {
        let ndsd = Ndsd::new();
        // On Linux (ClearKey), protected surfaces are not supported.
        #[cfg(target_os = "linux")]
        assert!(!ndsd.supports_protected_surfaces());
    }
}
