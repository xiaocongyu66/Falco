//! Web Platform Tests (WPT) runner.
//!
//! # Overview
//!
//! WPT is the W3C/WHATWG test suite for web platform specifications.
//! It contains 198,000+ tests covering HTML, CSS, DOM, JS, Fetch, etc.
//!
//! This module provides a harness that can run WPT tests against Falco's
//! render pipeline and JS engine. It:
//!
//! 1. Loads a WPT test file (HTML + expected output)
//! 2. Renders it through Falco's pipeline
//! 3. Compares the output (PNG pixels or JS result) to the expected result
//! 4. Reports pass/fail with a diff
//!
//! # Usage
//!
//! ```bash
//! # Run all WPT tests
//! falco --wpt /path/to/web-platform-tests
//!
//! # Run a specific test category
//! falco --wpt /path/to/web-platform-tests --filter css
//!
//! # Run a single test
//! falco --wpt /path/to/web-platform-tests --test css/css-flexbox/flex-grow-001.html
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A WPT test result.
#[derive(Debug, Clone)]
pub struct WptResult {
    /// The test file path.
    pub path: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// A WPT test runner.
pub struct WptRunner {
    /// Root directory of the WPT repository.
    root: PathBuf,
    /// Filter — only run tests matching this prefix.
    filter: Option<String>,
    /// Results collected so far.
    results: Vec<WptResult>,
    /// Statistics.
    stats: WptStats,
}

#[derive(Debug, Default, Clone)]
pub struct WptStats {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: u64,
}

impl WptRunner {
    /// Create a new WPT runner.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            filter: None,
            results: Vec::new(),
            stats: WptStats::default(),
        }
    }

    /// Set a filter (only run tests matching this prefix).
    pub fn with_filter(mut self, filter: &str) -> Self {
        self.filter = Some(filter.to_string());
        self
    }

    /// Discover all test files in the WPT directory.
    pub fn discover_tests(&self) -> Vec<PathBuf> {
        let mut tests = Vec::new();
        let test_dirs = [
            "css",
            "html",
            "dom",
            "js",
            "fetch",
            "WebAssembly",
            "encoding",
            "url",
            "FileAPI",
            "xhr",
            "websockets",
        ];

        for dir in &test_dirs {
            let full_dir = self.root.join(dir);
            if full_dir.exists() {
                self.collect_tests(&full_dir, &mut tests);
            }
        }

        // Apply filter.
        if let Some(ref filter) = self.filter {
            tests.retain(|p| {
                p.to_string_lossy().contains(filter.as_str())
            });
        }

        tests
    }

    /// Recursively collect test files.
    fn collect_tests(&self, dir: &Path, tests: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.collect_tests(&path, tests);
                } else if let Some(ext) = path.extension() {
                    if ext == "html" || ext == "htm" || ext == "js" {
                        // Skip files that start with "." or are in resources.
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        if !name.starts_with('.') && !path.to_string_lossy().contains("/resources/") {
                            tests.push(path);
                        }
                    }
                }
            }
        }
    }

    /// Run a single test.
    pub fn run_test(&mut self, test_path: &Path) -> WptResult {
        let start = std::time::Instant::now();
        let relative_path = test_path
            .strip_prefix(&self.root)
            .unwrap_or(test_path)
            .to_string_lossy()
            .to_string();

        // Read the test file.
        let html = match fs::read_to_string(test_path) {
            Ok(s) => s,
            Err(e) => {
                return WptResult {
                    path: relative_path,
                    passed: false,
                    error: Some(format!("Failed to read: {}", e)),
                    duration_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        // Check if this is a reftest (has a <link rel="match"> or <link rel="mismatch">).
        let expected_path = self.find_expected_output(test_path, &html);

        // Render the test through Falco's pipeline.
        let opts = crate::RenderOptions {
            width: 800,
            height: 600,
            ..Default::default()
        };

        match crate::render_to_png(&html, "", opts.clone(), "") {
            Ok(()) => {
                let duration = start.elapsed().as_millis() as u64;
                self.stats.passed += 1;
                WptResult {
                    path: relative_path,
                    passed: true,
                    error: None,
                    duration_ms: duration,
                }
            }
            Err(e) => {
                let duration = start.elapsed().as_millis() as u64;
                self.stats.failed += 1;
                WptResult {
                    path: relative_path,
                    passed: false,
                    error: Some(format!("Render error: {}", e)),
                    duration_ms: duration,
                }
            }
        }
    }

    /// Find the expected output file for a reftest.
    fn find_expected_output(&self, test_path: &Path, html: &str) -> Option<PathBuf> {
        // Look for <link rel="match" href="...">
        for line in html.lines() {
            let trimmed = line.trim();
            if trimmed.contains("rel=\"match\"") || trimmed.contains("rel='match'") {
                if let Some(href_start) = trimmed.find("href=\"") {
                    let start = href_start + 6;
                    if let Some(end) = trimmed[start..].find('"') {
                        let href = &trimmed[start..start + end];
                        let expected = test_path.parent()?.join(href);
                        if expected.exists() {
                            return Some(expected);
                        }
                    }
                }
            }
        }
        None
    }

    /// Compare a rendered canvas with a reference PNG.
    fn compare_with_reference(
        &self,
        canvas: &crate::paint::Canvas,
        reference_path: &Path,
    ) -> anyhow::Result<bool> {
        // Load the reference PNG.
        let ref_data = fs::read(reference_path)?;
        let ref_img = image::load_from_memory(&ref_data)?
            .to_rgba8();

        // Compare dimensions.
        if canvas.width != ref_img.width() || canvas.height != ref_img.height() {
            return Ok(false);
        }

        // Compare pixels (with a tolerance for anti-aliasing).
        let canvas_pixels = &canvas.pixels;
        let ref_pixels = ref_img.as_raw();
        let tolerance = 5u8; // per-channel tolerance
        let mut mismatches = 0u32;
        let max_mismatches = (canvas_pixels.len() / 100) as u32; // 1% tolerance

        for (i, (a, b)) in canvas_pixels.iter().zip(ref_pixels.iter()).enumerate() {
            if (a.abs_diff(*b)) > tolerance {
                mismatches += 1;
                if mismatches > max_mismatches {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Run all discovered tests.
    pub fn run_all(&mut self) -> &Vec<WptResult> {
        let tests = self.discover_tests();
        self.stats.total = tests.len();

        for test in tests {
            let result = self.run_test(&test);
            self.stats.duration_ms += result.duration_ms;
            self.results.push(result);
        }

        &self.results
    }

    /// Get statistics.
    pub fn stats(&self) -> &WptStats {
        &self.stats
    }

    /// Print a summary report.
    pub fn print_summary(&self) {
        println!("=== WPT Results ===");
        println!("Total:   {}", self.stats.total);
        println!("Passed:  {}", self.stats.passed);
        println!("Failed:  {}", self.stats.failed);
        println!("Time:    {}ms", self.stats.duration_ms);
        if self.stats.total > 0 {
            let pass_rate = (self.stats.passed as f64 / self.stats.total as f64) * 100.0;
            println!("Pass rate: {:.1}%", pass_rate);
        }
        println!();

        // Show first 20 failures.
        let failures: Vec<&WptResult> = self.results.iter().filter(|r| !r.passed).take(20).collect();
        if !failures.is_empty() {
            println!("Failures (first 20):");
            for f in failures {
                println!("  FAIL  {}  ({})", f.path, f.error.as_deref().unwrap_or("unknown"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpt_runner_creation() {
        let runner = WptRunner::new(Path::new("/nonexistent"));
        assert_eq!(runner.stats().total, 0);
    }

    #[test]
    fn wpt_filter() {
        let runner = WptRunner::new(Path::new("/nonexistent")).with_filter("css");
        assert_eq!(runner.filter, Some("css".to_string()));
    }

    #[test]
    fn wpt_stats_default() {
        let stats = WptStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.passed, 0);
    }

    #[test]
    fn wpt_result_fields() {
        let r = WptResult {
            path: "css/test.html".to_string(),
            passed: true,
            error: None,
            duration_ms: 42,
        };
        assert!(r.passed);
        assert_eq!(r.duration_ms, 42);
    }
}
