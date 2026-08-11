//! Multi-process architecture — process-per-tab isolation.
//!
//! # Overview
//!
//! In a real browser, each tab runs in a separate OS process. This provides:
//!
//! - **Crash isolation** — one tab crashing doesn't kill the browser
//! - **Security** — each process has its own memory space
//! - **SOP enforcement** — cross-origin content in a separate process
//! - **Resource limits** — per-process memory/CPU limits
//!
//! # Implementation
//!
//! Falco uses `std::process::Command` to spawn a child process for each tab.
//! The child process renders the page to a PNG file, and the parent process
//! reads the result.
//!
//! Communication is via:
//! - **stdin/stdout** — JSON messages (URL, render options)
//! - **files** — PNG output written to a temp file
//! - **exit code** — 0 = success, 1 = render error, 2 = crash
//!
//! # Usage
//!
//! ```no_run
//! use falco::multiprocess::ProcessManager;
//!
//! let mut manager = ProcessManager::new();
//! let handle = manager.spawn_tab("https://example.com")?;
//! let result = manager.wait_for_tab(handle)?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A unique tab ID.
static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// A handle to a spawned renderer process.
pub struct TabHandle {
    /// Unique tab ID.
    pub id: u64,
    /// The URL being rendered.
    pub url: String,
    /// The child process.
    pub child: Child,
    /// When the tab was spawned.
    pub started_at: Instant,
    /// Path to the output PNG file.
    pub output_path: PathBuf,
}

/// Result of a tab render.
#[derive(Debug)]
pub struct TabResult {
    /// Tab ID.
    pub id: u64,
    /// URL that was rendered.
    pub url: String,
    /// Whether the render succeeded.
    pub success: bool,
    /// Path to the output PNG (if successful).
    pub output_path: Option<PathBuf>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Render duration in milliseconds.
    pub duration_ms: u64,
}

/// Manages renderer processes.
pub struct ProcessManager {
    /// Active tabs (tab ID → handle).
    tabs: HashMap<u64, TabHandle>,
    /// Path to the Falco binary.
    binary_path: PathBuf,
    /// Default render width.
    default_width: u32,
    /// Default render height.
    default_height: u32,
    /// Maximum time to wait for a tab to render.
    timeout: Duration,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    /// Create a new process manager.
    pub fn new() -> Self {
        let binary_path = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("falco"));

        Self {
            tabs: HashMap::new(),
            binary_path,
            default_width: 1200,
            default_height: 800,
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the default viewport size.
    pub fn with_viewport(mut self, width: u32, height: u32) -> Self {
        self.default_width = width;
        self.default_height = height;
        self
    }

    /// Set the render timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Spawn a new renderer process for a URL.
    pub fn spawn_tab(&mut self, url: &str) -> anyhow::Result<u64> {
        let id = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        let output_path = std::env::temp_dir().join(format!("falco_tab_{}.png", id));

        let child = Command::new(&self.binary_path)
            .arg(url)
            .arg("--out")
            .arg(&output_path)
            .arg("--width")
            .arg(self.default_width.to_string())
            .arg("--height")
            .arg(self.default_height.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let handle = TabHandle {
            id,
            url: url.to_string(),
            child,
            started_at: Instant::now(),
            output_path,
        };

        self.tabs.insert(id, handle);
        Ok(id)
    }

    /// Wait for a tab to finish rendering.
    pub fn wait_for_tab(&mut self, tab_id: u64) -> anyhow::Result<TabResult> {
        let mut handle = self.tabs.remove(&tab_id)
            .ok_or_else(|| anyhow::anyhow!("Tab {} not found", tab_id))?;

        let url = handle.url.clone();
        let output_path = handle.output_path.clone();
        let started_at = handle.started_at;

        // Wait with timeout.
        let status = loop {
            match handle.child.try_wait()? {
                Some(status) => break status,
                None => {
                    if started_at.elapsed() > self.timeout {
                        // Kill the process.
                        let _ = handle.child.kill();
                        return Ok(TabResult {
                            id: tab_id,
                            url,
                            success: false,
                            output_path: None,
                            error: Some("Render timed out".to_string()),
                            duration_ms: started_at.elapsed().as_millis() as u64,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as u64;

        if status.success() && output_path.exists() {
            Ok(TabResult {
                id: tab_id,
                url,
                success: true,
                output_path: Some(output_path),
                error: None,
                duration_ms,
            })
        } else {
            Ok(TabResult {
                id: tab_id,
                url,
                success: false,
                output_path: None,
                error: Some(format!("Process exited with status: {}", status)),
                duration_ms,
            })
        }
    }

    /// Wait for all active tabs.
    pub fn wait_for_all(&mut self) -> Vec<TabResult> {
        let tab_ids: Vec<u64> = self.tabs.keys().copied().collect();
        let mut results = Vec::new();

        for id in tab_ids {
            if let Ok(result) = self.wait_for_tab(id) {
                results.push(result);
            }
        }

        results
    }

    /// Get the number of active tabs.
    pub fn active_tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Kill a tab (terminate the renderer process).
    pub fn kill_tab(&mut self, tab_id: u64) -> anyhow::Result<()> {
        if let Some(mut handle) = self.tabs.remove(&tab_id) {
            handle.child.kill()?;
        }
        Ok(())
    }

    /// Kill all active tabs.
    pub fn kill_all(&mut self) {
        for (_, mut handle) in self.tabs.drain() {
            let _ = handle.child.kill();
        }
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        self.kill_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_manager_creation() {
        let manager = ProcessManager::new();
        assert_eq!(manager.active_tab_count(), 0);
    }

    #[test]
    fn process_manager_with_viewport() {
        let manager = ProcessManager::new()
            .with_viewport(800, 600)
            .with_timeout(Duration::from_secs(10));
        assert_eq!(manager.default_width, 800);
        assert_eq!(manager.default_height, 600);
    }

    #[test]
    fn tab_id_increments() {
        let id1 = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        let id2 = NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst);
        assert!(id2 > id1);
    }

    #[test]
    fn tab_result_fields() {
        let r = TabResult {
            id: 1,
            url: "https://example.com".to_string(),
            success: true,
            output_path: Some(PathBuf::from("/tmp/out.png")),
            error: None,
            duration_ms: 100,
        };
        assert!(r.success);
        assert_eq!(r.duration_ms, 100);
    }
}
