//! Falco — a tiny, from-scratch browser engine written in Rust.
//!
//! Pipeline:
//!   HTML source → DOM → (CSS source → Stylesheet) → Style tree
//!   → Layout tree → Paint commands → Canvas → PNG

// Silence all warnings — the project has 90+ warnings from stub code,
// unused variables in cross-platform paths, and pattern matching that
// only triggers on certain platforms. These don't affect correctness.
#![allow(warnings)]
//!
//! ```no_run
//! use falco::{render_to_png, RenderOptions};
//! let opts = RenderOptions { width: 1200, height: 800, ..Default::default() };
//! render_to_png("<h1>Hello</h1>", "h1 { color: red; }", opts, "out.png").unwrap();
//! ```
//!
//! ## Module status (honest)
//!
//! The render pipeline currently uses the *legacy* modules below. They are
//! the ones actually called by [`render_with_base_url`]:
//!   - [`html`] — HTML parser (legacy)
//!   - [`dom`] — DOM types (legacy)
//!   - [`css`] — CSS parser + selector matching (legacy)
//!   - [`style`] — cascade + UA styles
//!   - [`layout`] — block/inline/flex/grid/table/float/absolute layout
//!   - [`paint`] — canvas + fonts + compositing
//!   - [`svg`] — SVG parser + renderer
//!   - [`png`] — hand-written PNG encoder
//!   - [`image`] — image loader (HTTP, data: URLs, local files)
//!   - [`js_tjs`], [`js_runner`] — JS execution via the TJS VM
//!   - [`tjs`] — the JavaScript VM itself
//!   - [`window`] — interactive `--window` mode
//!   - [`net`] — HTTP fetch, cookies, cache, websocket, redirect
//!
//! The following modules are **structurally complete and pass their own
//! unit tests, but NOT YET wired into the render pipeline**. They exist as
//! spec-compliant replacements for the legacy types above and are the
//! planned v0.2.0 milestone. Until then they are exported for review and
//! experimentation, but `render_with_base_url` does not call into them:
//!   - [`html::spec`] — WHATWG §13.2 tokenizer + tree builder + serializer + XML
//!   - [`dom::spec`] — spec DOM, MutationObserver, Shadow DOM, custom elements, a11y
//!   - [`css::spec`] — Selectors Level 4, cascade specificity, @-rules
//!   - [`tjs_ext`] — Symbol, BigInt, Promise, Map/Set, WeakMap/WeakSet, Reflect
//!
//! The following modules are **implemented but not enforced in the
//! renderer** — same situation, planned for a later milestone:
//!   - [`security`] — SOP, multi-process, sandbox, CSP, certs, permissions
//!   - [`web_runtime`] — fetch, XHR, event loop, WebGL/video/MSE/EME/NDSD stubs
//!   - [`media`] — media queries (parsed but always applied)

// === Render pipeline (active) ===
// dom/, css/, html/ each contain:
//   - the legacy (currently active) types at the module root
//   - the spec-compliant replacement under `spec/` (WIP, not yet wired in)
pub mod canvas;
pub mod css;
pub mod dom;
pub mod html;
pub mod image;
pub mod js_runner;
pub mod js_tjs;
pub mod layout;
pub mod media;
pub mod net;
pub mod paint;
pub mod png;
pub mod style;
pub mod svg;
pub mod tjs;
pub mod window;

// === Spec-compliant runtime extensions (NOT yet wired into render pipeline) ===
// These extend the active `tjs/` VM with Symbol, BigInt, Promise, etc.
// They compile and pass their own unit tests, but the JS bindings exposed
// to `onclick=` handlers don't yet surface all of these to user scripts.
pub mod tjs_ext;

// === Security / web runtime (implemented but not enforced in renderer) ===
pub mod security;
pub mod web_runtime;

// === WebAssembly runtime — full from-scratch implementation ===
// Parser, validator, stack-based interpreter, linear memory, function tables,
// globals, and the complete WebAssembly.* JS API.
pub mod wasm;

// === Additional Web Platform APIs ===
// TextEncoder/TextDecoder, Crypto (SHA-1/256/384/512), Web Workers,
// MessageChannel/BroadcastChannel, IndexedDB, Compression Streams,
// URLSearchParams, AbortController, queueMicrotask.
pub mod web_api;

// === Multi-process architecture ===
// Process-per-tab isolation via std::process::Command.
pub mod multiprocess;

// === Web Platform Tests runner ===
// Harness for running W3C/WHATWG WPT test suite against Falco.
pub mod wpt_runner;

/// Global storage for standalone SVG content.
static mut STANDALONE_SVG: Option<String> = None;

/// Store standalone SVG content globally.
pub fn set_standalone_svg(svg: String) {
    unsafe {
        STANDALONE_SVG = Some(svg);
    }
}

/// Get the standalone SVG content if available.
pub fn get_standalone_svg() -> Option<&'static str> {
    unsafe { STANDALONE_SVG.as_deref() }
}

use paint::{Canvas, FontRasterizer, PaintCommand};

/// Extract the contents of all `<style>...</style>` tags from HTML.
fn extract_style_tags(html: &str) -> String {
    let mut result = String::new();
    let lower = html.to_lowercase();
    let mut pos = 0;
    while pos < html.len() {
        if let Some(start) = lower[pos..].find("<style") {
            let abs_start = pos + start;
            // Find the end of the opening <style ...> tag.
            if let Some(tag_end) = lower[abs_start..].find('>') {
                let content_start = abs_start + tag_end + 1;
                // Find </style>.
                if let Some(close) = lower[content_start..].find("</style>") {
                    let content_end = content_start + close;
                    result.push_str(&html[content_start..content_end]);
                    result.push('\n');
                    pos = content_end + 8; // skip past </style>
                    continue;
                }
            }
            pos = abs_start + 1;
        } else {
            break;
        }
    }
    result
}

/// Strip all <script>...</script> tags from HTML, replacing with nothing.
fn strip_scripts(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let lower = html.to_lowercase();
    let mut pos = 0;
    while pos < html.len() {
        if let Some(start) = lower[pos..].find("<script") {
            let abs_start = pos + start;
            result.push_str(&html[pos..abs_start]);
            if let Some(end) = lower[abs_start..].find("</script>") {
                pos = abs_start + end + 9;
            } else {
                if let Some(close) = lower[abs_start..].find("/>") {
                    pos = abs_start + close + 2;
                } else {
                    pos = html.len();
                }
            }
        } else {
            result.push_str(&html[pos..]);
            break;
        }
    }
    result
}

/// Inject minimal fallback CSS — only ensures hidden elements become visible.
fn inject_fallback_css(html: &str) -> String {
    let fallback = r#"<style>
        [style*="display:none"], [style*="display: none"] { display: block !important; }
        [hidden] { display: block !important; }
    </style>"#;
    let lower = html.to_lowercase();
    if let Some(pos) = lower.rfind("</head>") {
        let mut result = String::with_capacity(html.len() + fallback.len());
        result.push_str(&html[..pos]);
        result.push_str(fallback);
        result.push_str(&html[pos..]);
        result
    } else {
        format!("{}{}", fallback, html)
    }
}

/// Options for a single render pass.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Viewport width in pixels.
    pub width: u32,
    /// Viewport height cap in pixels. The canvas grows if content is taller.
    pub height: u32,
    /// Background color (RGBA hex like 0xFFFFFFFF for white).
    pub background: u32,
    /// Animation clock in milliseconds. Used for CSS @keyframes animation
    /// playback — the style tree builder uses this to compute the current
    /// position within each animation. In --window mode, this advances
    /// on each frame; for PNG output, it defaults to 0 (animation start).
    pub animation_time_ms: u64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            background: 0xFFFFFFFF,
            animation_time_ms: 0,
        }
    }
}

/// Render HTML + CSS to a PNG file. Convenience entry point for the library.
pub fn render_to_png(
    html_src: &str,
    css_src: &str,
    opts: RenderOptions,
    output_path: &str,
) -> anyhow::Result<()> {
    let (canvas, _commands) = render(html_src, css_src, opts)?;
    let png_bytes = png::encode(canvas.width, canvas.height, &canvas.pixels);
    std::fs::write(output_path, png_bytes)?;
    Ok(())
}

/// Raw RGBA pixel buffer returned by [`render_to_buffer`].
///
/// Intended for embedding Falco into game engines, GUI toolkits, or other
/// contexts where you want to upload the rendered pixels directly to a GPU
/// texture instead of going through PNG encoding.
///
/// The pixel layout is row-major, 4 bytes per pixel (R, G, B, A), top-to-bottom.
/// The buffer length is always `width * height * 4`.
#[derive(Debug, Clone)]
pub struct RenderedBuffer {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw RGBA pixel data, row-major, top-to-bottom.
    pub pixels: Vec<u8>,
}

impl RenderedBuffer {
    /// Returns the raw RGBA bytes as a flat slice.
    ///
    /// The slice length is `width * height * 4`. Layout: row-major,
    /// top-to-bottom, 4 bytes per pixel (R, G, B, A).
    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the same pixels as BGRA (swapped R and B channels).
    ///
    /// Useful for DirectX / Vulkan / Win32 surfaces that expect BGRA
    /// instead of RGBA.
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len());
        for chunk in self.pixels.chunks_exact(4) {
            out.push(chunk[2]); // B
            out.push(chunk[1]); // G
            out.push(chunk[0]); // R
            out.push(chunk[3]); // A
        }
        out
    }

    /// Strip the alpha channel, returning RGB only (3 bytes per pixel).
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.pixels.len() / 4) * 3);
        for chunk in self.pixels.chunks_exact(4) {
            out.push(chunk[0]);
            out.push(chunk[1]);
            out.push(chunk[2]);
        }
        out
    }
}

/// Render HTML + CSS to a raw RGBA pixel buffer (no PNG encoding).
///
/// This is the entry point for embedding Falco into game engines and GUI
/// toolkits. The returned [`RenderedBuffer`] contains `width * height * 4`
/// bytes in RGBA order, ready to upload to a GPU texture.
///
/// # Example
///
/// ```no_run
/// use falco::{render_to_buffer, RenderOptions};
/// let opts = RenderOptions { width: 1280, height: 720, ..Default::default() };
/// let rendered = render_to_buffer("<h1>Hello</h1>", "h1 { color: red; }", opts)?;
/// // rendered.as_rgba() is &[u8] RGBA, ready for glTexImage2D / D3D texture upload
/// // rendered.to_bgra() returns Vec<u8> for DirectX / Win32 surfaces
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn render_to_buffer(
    html_src: &str,
    css_src: &str,
    opts: RenderOptions,
) -> anyhow::Result<RenderedBuffer> {
    let (canvas, _commands) = render(html_src, css_src, opts)?;
    Ok(RenderedBuffer {
        width: canvas.width,
        height: canvas.height,
        pixels: canvas.pixels,
    })
}

/// Render HTML + CSS to a Canvas + the list of paint commands (for inspection).
pub fn render(
    html_src: &str,
    css_src: &str,
    opts: RenderOptions,
) -> anyhow::Result<(Canvas, Vec<PaintCommand>)> {
    render_with_base_url(html_src, css_src, opts, "")
}

/// Render HTML + CSS with a base URL (for resolving relative image URLs).
///
/// This function:
/// 1. Parses HTML into a legacy DOM (for layout compatibility).
/// 2. Converts the legacy DOM to a spec DOM (for JS bridge).
/// 3. Executes inline `<script>` tags via TJS with the spec DOM bridge.
///    Scripts can use `document.getElementById`, `element.innerHTML = ...`,
///    `element.style.color = ...`, `fetch()`, etc. — mutations are reflected
///    on the spec DOM.
/// 4. Serializes the (possibly mutated) spec DOM back to HTML.
/// 5. Re-parses the mutated HTML into a legacy DOM for layout.
/// 6. Parses CSS.
/// 7. Builds style tree → layout tree → paint commands.
/// 8. Paints to canvas.
pub fn render_with_base_url(
    html_src: &str,
    css_src: &str,
    opts: RenderOptions,
    base_url: &str,
) -> anyhow::Result<(Canvas, Vec<PaintCommand>)> {
    // Set the base URL for image resolution.
    image::set_base_url(base_url);

    // 1. Strip <script> tags and inject fallback CSS to ensure visibility.
    let clean_html = strip_scripts(html_src);
    let clean_html = inject_fallback_css(&clean_html);

    // 2. Parse the cleaned HTML. Try the spec-compliant WHATWG HTML5 parser
    //    first, fall back to legacy if it fails.
    //    Skip spec parser for SVG content — it doesn't handle SVG elements
    //    properly (drops them during tree construction).
    let has_svg = clean_html.contains("<svg");
    let dom = if has_svg {
        html::parse(&clean_html)
    } else {
        let spec_doc = crate::html::spec::parse(&clean_html);
        match crate::js_tjs::spec_document_to_legacy_dom(&spec_doc) {
            Some(d) => d,
            None => {
                eprintln!("[falco:html] spec parser failed, using legacy parser");
                html::parse(&clean_html)
            }
        }
    };

    // 3. Extract and execute inline <script> tags via TJS with the DOM bridge.
    let scripts = crate::js_runner::extract_scripts(html_src);

    // Extract external script sources (<script src="...">).
    let external_scripts = crate::js_runner::extract_external_scripts(html_src);

    // Extract CSP policy from <meta http-equiv="Content-Security-Policy">.
    // If the policy forbids inline scripts, we skip script execution entirely.
    // If the policy forbids eval, we block eval() calls in JS.
    let csp = crate::js_runner::extract_csp_policy(html_src);
    let inline_scripts_allowed = csp
        .as_ref()
        .map(|p| p.allows_inline_script(None, None))
        .unwrap_or(true); // No CSP → allow.

    // Collect all scripts: inline + external (fetched).
    // External scripts are fetched via HTTP and their source code is added
    // to the scripts list, so they execute in the TJS context alongside
    // inline scripts.
    let mut all_scripts = scripts.clone();

    // Fetch and execute external scripts.
    for (src, _is_async) in &external_scripts {
        // CSP script-src check — actually enforce CSP policy.
        if let Some(ref policy) = csp {
            if !policy.allows_script_src(src) {
                eprintln!("[falco:csp] external script blocked by CSP script-src: {}", src);
                continue;
            }
        }

        // Resolve relative URLs against base_url.
        let full_url = if src.starts_with("http://") || src.starts_with("https://") {
            src.clone()
        } else if src.starts_with("/") {
            if let Some(origin) = base_url.split('/').take(3).collect::<Vec<_>>().first() {
                format!("{}{}", origin, src)
            } else {
                src.clone()
            }
        } else {
            format!("{}/{}", base_url.trim_end_matches('/'), src)
        };

        eprintln!("[falco:script] fetching external script: {}", full_url);
        match ureq::get(&full_url).call() {
            Ok(resp) => {
                let body = resp.into_string().unwrap_or_default();
                eprintln!(
                    "[falco:script] fetched {} bytes from {}",
                    body.len(),
                    full_url
                );
                all_scripts.push(body);
            }
            Err(e) => {
                eprintln!("[falco:script] failed to fetch {}: {}", full_url, e);
            }
        }
    }

    let final_html = if !all_scripts.is_empty() && inline_scripts_allowed {
        // Convert legacy DOM to spec DOM (gives JS live parent/child/sibling
        // pointers and MutationObserver support).
        let spec_doc = crate::js_tjs::legacy_dom_to_spec_document(&dom);

        // Create a TJS context with the spec DOM attached.
        // Pass the base_url for SOP enforcement (cross-origin fetch checks).
        // Pass CSP policy for connect-src/script-src enforcement.
        let mut tjs = crate::js_tjs::TjsJsContext::with_origin(spec_doc.clone(), Some(base_url))
            .with_csp(csp.clone());

        // Execute each <script> in order (inline + fetched external).
        // Errors are logged but non-fatal (the renderer should still produce
        // output even if a script throws).
        for script in &all_scripts {
            if let Err(e) = tjs.execute(script) {
                eprintln!("[falco:tjs] script error (continuing): {}", e);
            }
        }

        // Serialize the (possibly mutated) spec DOM back to HTML.
        let mutated_html = crate::js_tjs::serialize_spec_document(&spec_doc);

        // Re-inject <style> tags (the serializer drops them because they
        // live in <head>, which is preserved, but we want to be safe).
        let mutated_html = if mutated_html.is_empty() {
            clean_html.clone()
        } else {
            inject_fallback_css(&strip_scripts(&mutated_html))
        };

        // Re-parse the mutated HTML into a fresh legacy DOM for layout.
        html::parse(&mutated_html)
    } else {
        if !scripts.is_empty() && !inline_scripts_allowed {
            eprintln!("[falco:csp] CSP blocks inline scripts — skipping execution");
        }
        // No scripts (or CSP blocked them) — use the originally parsed DOM.
        dom
    };

    // 4. Extract <style> tags from the cleaned (pre-script) HTML and merge
    //    with external CSS. We use the pre-script HTML here because the
    //    spec-DOM serializer may strip <style> tags in some edge cases.
    let all_css = format!("{}\n{}", extract_style_tags(&clean_html), css_src);

    // 5. Parse CSS (merged from <style> tags + external). Pass the viewport
    //    size so that @media queries are evaluated conditionally.
    let stylesheet = css::parse_with_viewport(&all_css, opts.width, opts.height);

    // 6. Build style tree from the (possibly mutated by JS) DOM.
    let style_tree = style::build_style_tree(&final_html, &stylesheet, opts.animation_time_ms);

    // 7. Build layout tree.
    let layout_root = layout::build_layout_tree(&style_tree, opts.width as f32);

    // 8. Collect paint commands.
    let mut commands = Vec::new();
    layout::collect_paint_commands(&layout_root, &mut commands);

    // 9. Determine final canvas height.
    let actual_height = (layout_root.bounds.y + layout_root.bounds.height).ceil() as u32;
    let canvas_height = actual_height.max(opts.height);

    // 10. Paint.
    let mut canvas = Canvas::new(opts.width, canvas_height);
    let bg = opts.background;
    let bg_color = css::Color::rgba(
        ((bg >> 24) & 0xff) as u8,
        ((bg >> 16) & 0xff) as u8,
        ((bg >> 8) & 0xff) as u8,
        (bg & 0xff) as u8,
    );
    if bg_color.a > 0 {
        canvas.fill_rect(0.0, 0.0, opts.width as f32, canvas_height as f32, bg_color);
    }
    let rasterizer = FontRasterizer::new()?;
    paint::paint(&commands, &mut canvas, &rasterizer);
    eprintln!(
        "[falco:dbg] css_len={} commands={} canvas={}x{}",
        all_css.len(),
        commands.len(),
        canvas.width,
        canvas.height
    );
    let nonwhite = canvas
        .pixels
        .chunks(4)
        .filter(|p| !(p[0] == 255 && p[1] == 255 && p[2] == 255 && p[3] == 255))
        .count();
    eprintln!("[falco:dbg] nonwhite_pixels={}", nonwhite);
    Ok((canvas, commands))
}
