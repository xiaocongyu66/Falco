//! Interactive window mode — a real browser session with navigation,
//! history, forms, scrolling, and incremental repaint.
//!
//! Keys:
//!   q / Esc       — quit
//!   r             — reload current page (re-fetch + re-render)
//!   ↑ / ↓         — scroll line
//!   PgUp/PgDn     — scroll page
//!   Home/End      — jump to top / bottom
//!   Tab/Shift+Tab — cycle focus between interactive elements
//!   Backspace     — delete char from focused input
//!   Enter         — submit form / click focused button / follow focused link
//!   Alt+←/→       — back / forward in history
//!   F6 / Ctrl+L   — focus the address bar
//!   letters       — type into focused input or address bar
//! Mouse:
//!   wheel         — scroll
//!   click link    — navigate
//!   click input   — focus
//!   click address bar — focus address bar

use crate::css::Color;
use crate::layout::{BoxContent, InlineItem, LayoutBox};
use crate::paint::{Canvas, FontRasterizer};
use minifb::{Key, KeyRepeat, MouseMode, ScaleMode, Window, WindowOptions};
use std::time::{Duration, Instant};

/// A page in the browser history.
#[derive(Clone)]
struct Page {
    url: String,
    html_src: String,
    css_src: String,
}

/// Interactive browser session.
pub struct Browser {
    window: Window,
    rasterizer: FontRasterizer,
    /// Width of the viewport (used for layout).
    viewport_width: u32,
    /// Background color.
    background: Color,
    /// The full-page canvas (potentially much taller than the window).
    page_canvas: Canvas,
    /// The visible window buffer (RGBA → u32 for minifb).
    window_buffer: Vec<u32>,
    window_width: usize,
    window_height: usize,
    scroll_y: usize,
    /// Layout tree for hit-testing.
    layout_root: LayoutBox,
    /// Navigation history.
    history: Vec<Page>,
    history_index: usize,
    /// Address bar input state.
    address_bar_text: String,
    address_bar_focused: bool,
    /// Index of the focused interactive element, plus its text input state.
    focused_element: Option<usize>,
    input_text: String,
    /// List of interactive elements found in the page.
    interactive_elements: Vec<InteractiveElement>,
    /// Cursor blink state.
    blink_on: bool,
    last_blink: Instant,
    /// Start time for CSS animation clock. Reset on each page load.
    animation_start: Instant,
    /// Dirty flag — set when the page needs a full re-paint.
    needs_full_repaint: bool,
    /// Status bar text.
    status: String,
    /// The current page's stylesheet (for animation clock detection).
    current_stylesheet: Option<crate::css::Stylesheet>,
    /// The current page's DOM (for animation re-render).
    current_dom: Option<crate::dom::Node>,
}

#[derive(Debug, Clone)]
struct InteractiveElement {
    id: usize,
    bounds: crate::layout::Rect,
    kind: InteractiveKind,
    href: Option<String>,
    initial_value: String,
    /// Inline event handlers extracted from the DOM (onclick, onload, etc.).
    /// Maps event name → JS source.
    event_handlers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
enum InteractiveKind {
    Link,
    Input,
    Button,
    Textarea,
    Checkbox,
}

/// Address bar geometry — fixed at the top of the window.
const ADDRESS_BAR_HEIGHT: usize = 32;
/// Status bar geometry — fixed at the bottom of the window.
const STATUS_BAR_HEIGHT: usize = 24;

impl Browser {
    /// Open a new browser window with the initial page loaded.
    pub fn open(
        title: &str,
        html_src: String,
        css_src: String,
        url: String,
        viewport_width: u32,
        background: Color,
    ) -> anyhow::Result<()> {
        let rasterizer = FontRasterizer::new()?;
        let window_width = viewport_width.min(1400).max(800) as usize;
        let window_height = 800usize;

        let mut window = Window::new(
            title,
            window_width,
            window_height,
            WindowOptions {
                resize: true,
                scale_mode: ScaleMode::UpperLeft,
                ..Default::default()
            },
        )?;
        window.limit_update_rate(Some(Duration::from_millis(16)));

        let window_buffer = vec![0xFFFFFFFFu32; window_width * window_height];

        let first_page = Page {
            url: url.clone(),
            html_src,
            css_src,
        };
        let mut browser = Browser {
            window,
            rasterizer,
            viewport_width,
            background,
            page_canvas: Canvas::new(viewport_width, window_height as u32),
            window_buffer,
            window_width,
            window_height,
            scroll_y: 0,
            layout_root: LayoutBox {
                style: crate::style::ComputedStyle::default(),
                bounds: crate::layout::Rect::default(),
                content: BoxContent::Empty,
                tag: None,
                attrs: Default::default(),
                element_id: None,
            },
            history: vec![first_page.clone()],
            history_index: 0,
            address_bar_text: url,
            address_bar_focused: false,
            focused_element: None,
            input_text: String::new(),
            interactive_elements: Vec::new(),
            blink_on: true,
            last_blink: Instant::now(),
            animation_start: Instant::now(),
            needs_full_repaint: true,
            status: String::new(),
            current_stylesheet: None,
            current_dom: None,
        };

        // Render the initial page.
        browser.load_page(&first_page)?;
        browser.run()?;
        Ok(())
    }

    /// Render a page from its HTML/CSS source. Updates layout_root,
    /// page_canvas, and interactive_elements.
    fn load_page(&mut self, page: &Page) -> anyhow::Result<()> {
        // Reset animation clock on page load.
        self.animation_start = Instant::now();

        // Set the base URL for image resolution.
        // Strip URL fragment (#...) — it's client-side only.
        let clean_url = if let Some(pos) = page.url.find('#') {
            page.url[..pos].to_string()
        } else {
            page.url.clone()
        };
        crate::image::set_base_url(&clean_url);

        // Detect standalone SVG content and wrap it in HTML.
        let html_src = if page.html_src.trim_start().starts_with("<?xml")
            || page
                .html_src
                .trim_start()
                .to_lowercase()
                .starts_with("<svg")
        {
            eprintln!("[falco:window] detected standalone SVG, wrapping in HTML");
            let svg_content = if page.html_src.trim_start().starts_with("<?xml") {
                page.html_src
                    .split("?>")
                    .nth(1)
                    .unwrap_or(&page.html_src)
                    .trim_start()
            } else {
                page.html_src.trim_start()
            };
            crate::set_standalone_svg(svg_content.to_string());
            format!(
                "<!DOCTYPE html>\n<html><head><style>\n\
                 body {{ margin: 0; padding: 0; background: white; }}\n\
                 svg {{ display: block; }}\n\
                 </style></head><body>\n\
                 <svg data-falco-standalone=\"true\"></svg>\n\
                 </body></html>"
            )
        } else {
            page.html_src.clone()
        };

        // Strip <script> tags (they break the legacy parser on large pages).
        let clean_html = strip_scripts_window(&html_src);
        // Inject fallback CSS for visibility.
        let clean_html = inject_fallback_css_window(&clean_html);
        // Extract <style> tags and merge with external CSS.
        let style_css = extract_style_tags_window(&clean_html);
        let all_css = format!("{}\n{}", style_css, page.css_src);

        // Use legacy parser for SVG content (spec parser drops SVG elements).
        let dom = if clean_html.contains("<svg") {
            crate::html::parse(&clean_html)
        } else {
            crate::html::parse(&clean_html)
        };
        let stylesheet = crate::css::parse_with_viewport(&all_css, self.viewport_width, 800);

        // Save for animation re-rendering.
        self.current_stylesheet = Some(stylesheet.clone());
        self.current_dom = Some(dom.clone());

        // Compute animation time from the page-load start.
        let animation_time_ms = self.animation_start.elapsed().as_millis() as u64;
        let style_tree = crate::style::build_style_tree(&dom, &stylesheet, animation_time_ms);
        let layout_root = crate::layout::build_layout_tree(&style_tree, self.viewport_width as f32);
        let mut commands = Vec::new();
        crate::layout::collect_paint_commands(&layout_root, &mut commands);

        let actual_height = (layout_root.bounds.y + layout_root.bounds.height).ceil() as u32;
        let canvas_height = actual_height.max(400);
        let mut canvas = Canvas::new(self.viewport_width, canvas_height);
        if self.background.a > 0 {
            canvas.fill_rect(
                0.0,
                0.0,
                self.viewport_width as f32,
                canvas_height as f32,
                self.background,
            );
        }
        crate::paint::paint(&commands, &mut canvas, &self.rasterizer);

        self.page_canvas = canvas;
        self.layout_root = layout_root;
        self.scroll_y = 0;
        self.focused_element = None;
        self.input_text.clear();
        self.address_bar_text = page.url.clone();

        // Scan the layout tree for interactive elements.
        let mut interactive_elements = Vec::new();
        let mut scanner = ElementScanner {
            elements: &mut interactive_elements,
            next_id: 1,
        };
        scanner.scan(&self.layout_root);
        self.interactive_elements = interactive_elements;

        // Execute JavaScript via TJS (our own engine with window/document stubs).
        let scripts = crate::js_runner::extract_scripts(&page.html_src);
        if !scripts.is_empty() {
            let mut tjs = crate::tjs::TjsContext::new();
            for script in &scripts {
                if let Err(e) = tjs.execute(script) {
                    eprintln!("[falco:tjs] script error: {}", e);
                }
            }
        }

        self.needs_full_repaint = true;
        self.update_status();
        Ok(())
    }

    /// Fetch a URL and return its HTML. For `http(s)://` URLs we use ureq.
    /// For relative URLs (e.g. `/foo`), we resolve against the current page.
    fn fetch_url(&self, url: &str, base: &str) -> anyhow::Result<(String, String)> {
        let resolved = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else if url.starts_with("/") {
            // Absolute path — resolve against base URL's origin.
            if let Some(origin_end) = base
                .find("://")
                .and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j))
            {
                base[..origin_end].to_string() + url
            } else {
                format!("{}{}", base, url)
            }
        } else if url.starts_with("#") {
            // Anchor — same page, just jump.
            base.to_string()
        } else {
            // Relative path — append to base.
            let last_slash = base.rfind('/').unwrap_or(base.len());
            format!("{}{}", &base[..last_slash + 1], url)
        };

        // Strip URL fragment (#...) — it's client-side only, not sent to server.
        let fetch_url = if let Some(pos) = resolved.find('#') {
            resolved[..pos].to_string()
        } else {
            resolved.clone()
        };

        eprintln!("[falco] fetching: {}", fetch_url);
        let html = crate::net::fetch(&fetch_url)?;
        let css = extract_style_tags(&html);
        Ok((html, css))
    }

    /// Navigate to a new URL. Fetches the page, loads it, and pushes onto history.
    fn navigate(&mut self, url: &str) {
        let current = &self.history[self.history_index];
        match self.fetch_url(url, &current.url) {
            Ok((html, css)) => {
                let page = Page {
                    url: url.to_string(),
                    html_src: html,
                    css_src: css,
                };
                // Truncate forward history.
                if self.history_index + 1 < self.history.len() {
                    self.history.truncate(self.history_index + 1);
                }
                self.history.push(page.clone());
                self.history_index = self.history.len() - 1;
                if let Err(e) = self.load_page(&page) {
                    self.status = format!("Error loading page: {e}");
                }
            }
            Err(e) => {
                self.status = format!("Failed to load {}: {e}", url);
                eprintln!("[falco] error: {e}");
            }
        }
    }

    /// Reload the current page.
    fn reload(&mut self) {
        let page = self.history[self.history_index].clone();
        eprintln!("[falco] reloading: {}", page.url);
        match self.fetch_url(&page.url, &page.url) {
            Ok((html, css)) => {
                let new_page = Page {
                    url: page.url.clone(),
                    html_src: html,
                    css_src: css,
                };
                self.history[self.history_index] = new_page.clone();
                if let Err(e) = self.load_page(&new_page) {
                    self.status = format!("Error reloading: {e}");
                }
            }
            Err(e) => {
                self.status = format!("Reload failed: {e}");
            }
        }
    }

    fn go_back(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
            let page = self.history[self.history_index].clone();
            let _ = self.load_page(&page);
            eprintln!("[falco] back to: {}", page.url);
        }
    }

    fn go_forward(&mut self) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let page = self.history[self.history_index].clone();
            let _ = self.load_page(&page);
            eprintln!("[falco] forward to: {}", page.url);
        }
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let mut last_hover_check = Instant::now();
        let mut prev_mouse_down = false;
        let mut prev_input_text = self.input_text.clone();
        let mut prev_scroll = self.scroll_y;
        let mut prev_focused = self.focused_element;

        while self.window.is_open()
            && !self.window.is_key_down(Key::Escape)
            && !self.window.is_key_down(Key::Q)
        {
            // Handle navigation keys.
            if self.window.is_key_pressed(Key::R, KeyRepeat::No) {
                self.reload();
            }
            if self.window.is_key_pressed(Key::Left, KeyRepeat::No)
                && (self.window.is_key_down(Key::LeftAlt) || self.window.is_key_down(Key::RightAlt))
            {
                self.go_back();
            }
            if self.window.is_key_pressed(Key::Right, KeyRepeat::No)
                && (self.window.is_key_down(Key::LeftAlt) || self.window.is_key_down(Key::RightAlt))
            {
                self.go_forward();
            }

            // Address bar focus.
            if self.window.is_key_pressed(Key::F6, KeyRepeat::No)
                || self.window.is_key_pressed(Key::F6, KeyRepeat::No)
            {
                self.address_bar_focused = true;
                self.focused_element = None;
            }
            if self.window.is_key_pressed(Key::Escape, KeyRepeat::No) && self.address_bar_focused {
                self.address_bar_focused = false;
            }

            // Scroll keys (disabled when address bar is focused).
            if !self.address_bar_focused {
                if self.window.is_key_pressed(Key::Down, KeyRepeat::Yes) {
                    self.scroll_by(60);
                }
                if self.window.is_key_pressed(Key::Up, KeyRepeat::Yes) {
                    self.scroll_by(-60);
                }
                if self.window.is_key_pressed(Key::PageDown, KeyRepeat::Yes) {
                    self.scroll_by(self.window_height as isize);
                }
                if self.window.is_key_pressed(Key::PageUp, KeyRepeat::Yes) {
                    self.scroll_by(-(self.window_height as isize));
                }
                if self.window.is_key_pressed(Key::Home, KeyRepeat::No) {
                    self.scroll_y = 0;
                    self.update_status();
                }
                if self.window.is_key_pressed(Key::End, KeyRepeat::No) {
                    self.scroll_y = self.max_scroll();
                    self.update_status();
                }
            }

            // Tab — cycle focus (unless address bar has focus).
            if !self.address_bar_focused && self.window.is_key_pressed(Key::Tab, KeyRepeat::No) {
                self.cycle_focus(
                    self.window.is_key_down(Key::LeftShift)
                        || self.window.is_key_down(Key::RightShift),
                );
            }

            // Backspace.
            if self.window.is_key_pressed(Key::Backspace, KeyRepeat::Yes) {
                if self.address_bar_focused {
                    self.address_bar_text.pop();
                } else if self.focused_element.is_some() && !self.input_text.is_empty() {
                    self.input_text.pop();
                    self.rerender_page();
                }
            }

            // Enter.
            if self.window.is_key_pressed(Key::Enter, KeyRepeat::No) {
                if self.address_bar_focused {
                    let url = self.address_bar_text.clone();
                    self.address_bar_focused = false;
                    self.navigate(&url);
                } else if let Some(focused_id) = self.focused_element {
                    if let Some(elem) = self
                        .interactive_elements
                        .iter()
                        .find(|e| e.id == focused_id)
                        .cloned()
                    {
                        match elem.kind {
                            InteractiveKind::Link => {
                                if let Some(href) = &elem.href {
                                    self.navigate(href);
                                }
                            }
                            InteractiveKind::Button | InteractiveKind::Input => {
                                if elem.kind == InteractiveKind::Input
                                    && elem.initial_value.is_empty()
                                    && self.input_text.is_empty()
                                {
                                    // Empty input — ignore.
                                } else {
                                    self.status =
                                        format!("Button clicked: {:?}", elem.initial_value);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Text input.
            let shift =
                self.window.is_key_down(Key::LeftShift) || self.window.is_key_down(Key::RightShift);
            let typed = self.window.get_keys_pressed(KeyRepeat::Yes);
            let mut typed_something = false;
            for key in typed {
                if let Some(c) = key_to_char(key, shift) {
                    if self.address_bar_focused {
                        self.address_bar_text.push(c);
                        typed_something = true;
                    } else if self.focused_element.is_some() {
                        self.input_text.push(c);
                        typed_something = true;
                    }
                }
            }
            if typed_something && self.focused_element.is_some() {
                self.rerender_page();
            }

            // Mouse wheel.
            let wheel = self.window.get_scroll_wheel();
            if let Some((dx, dy)) = wheel {
                let _ = dx;
                let delta = -(dy * 60.0) as isize;
                self.scroll_by(delta);
            }

            // Mouse position + click.
            let mouse = self.window.get_mouse_pos(MouseMode::Clamp);
            let (mx, my) = if let Some((x, y)) = mouse {
                (x as isize, y as isize)
            } else {
                (-1, -1)
            };

            // Hover check (throttled).
            if last_hover_check.elapsed() > Duration::from_millis(33) {
                self.check_hover(mx, my);
                last_hover_check = Instant::now();
            }

            // Click — falling edge.
            let mouse_down = self.window.get_mouse_down(minifb::MouseButton::Left);
            if mouse_down && !prev_mouse_down {
                self.handle_click(mx, my);
            }
            prev_mouse_down = mouse_down;

            // Blink cursor.
            if self.last_blink.elapsed() > Duration::from_millis(500) {
                self.blink_on = !self.blink_on;
                self.last_blink = Instant::now();
            }

            // CSS animations: re-render the page every 16ms (~60fps) to
            // advance the animation clock. This is a full re-layout + re-paint,
            // which is expensive — a production browser would do incremental
            // re-layout only for animated elements. For Falco's teaching scope
            // this is acceptable.
            //
            // We only do this if the page has @keyframes rules (detected by
            // checking if the stylesheet has any). Pages without animations
            // are not re-rendered, keeping CPU usage low.
            if let Some(ref stylesheet) = self.current_stylesheet {
                if !stylesheet.keyframes.is_empty() {
                    let elapsed = self.animation_start.elapsed();
                    if elapsed > Duration::from_millis(16) {
                        self.rerender_page_with_animation();
                    }
                }
            }

            // Determine whether we need to repaint.
            let input_changed = self.input_text != prev_input_text;
            let scroll_changed = self.scroll_y != prev_scroll;
            let focus_changed = self.focused_element != prev_focused;
            if self.needs_full_repaint || input_changed || scroll_changed || focus_changed {
                self.render_to_buffer();
                self.draw_address_bar();
                self.draw_focus_indicator();
                self.draw_status_bar();
                self.window.update_with_buffer(
                    &self.window_buffer,
                    self.window_width,
                    self.window_height,
                )?;
                self.needs_full_repaint = false;
            } else {
                // Just redraw address bar + status + focus (cheap overlays).
                self.draw_address_bar();
                self.draw_focus_indicator();
                self.draw_status_bar();
                self.window.update_with_buffer(
                    &self.window_buffer,
                    self.window_width,
                    self.window_height,
                )?;
            }

            prev_input_text = self.input_text.clone();
            prev_scroll = self.scroll_y;
            prev_focused = self.focused_element;
        }
        Ok(())
    }

    fn scroll_by(&mut self, delta: isize) {
        let new_scroll = self.scroll_y as isize + delta;
        self.scroll_y = new_scroll.max(0).min(self.max_scroll() as isize) as usize;
        self.update_status();
    }

    fn max_scroll(&self) -> usize {
        (self.page_canvas.height as usize)
            .saturating_sub(self.window_height - ADDRESS_BAR_HEIGHT - STATUS_BAR_HEIGHT)
    }

    fn update_status(&mut self) {
        let pct = if self.page_canvas.height > 0 {
            (self.scroll_y * 100 / (self.page_canvas.height as usize).max(1)).min(100)
        } else {
            0
        };
        if let Some(href) = &self.find_hovered_href() {
            self.status = format!("→ {}", href);
        } else {
            let url = &self.history[self.history_index].url;
            self.status = format!("{} — scroll: {}%", url, pct);
        }
    }

    fn find_hovered_href(&self) -> Option<String> {
        // We don't track the hovered element persistently in this version —
        // the status bar just shows the URL.
        None
    }

    /// Re-render the page canvas (called when input text changes).
    fn rerender_page(&mut self) {
        let width = self.page_canvas.width;
        let height = self.page_canvas.height;
        let bg = self.background;
        let mut canvas = Canvas::new(width, height);
        if bg.a > 0 {
            canvas.fill_rect(0.0, 0.0, width as f32, height as f32, bg);
        }

        // Patch the focused input's value.
        if let Some(focused_id) = self.focused_element {
            patch_input_value(&mut self.layout_root, focused_id, &self.input_text);
        }

        let mut commands = Vec::new();
        crate::layout::collect_paint_commands(&self.layout_root, &mut commands);
        crate::paint::paint(&commands, &mut canvas, &self.rasterizer);
        self.page_canvas = canvas;
        self.needs_full_repaint = true;
    }

    /// Re-render the page with the current animation time. This rebuilds
    /// the style tree and layout from scratch, using the elapsed time since
    /// `animation_start` as the animation clock. Called every ~16ms when
    /// the page has @keyframes rules.
    fn rerender_page_with_animation(&mut self) {
        let (dom, stylesheet) = match (&self.current_dom, &self.current_stylesheet) {
            (Some(d), Some(s)) => (d.clone(), s.clone()),
            _ => return,
        };

        let animation_time_ms = self.animation_start.elapsed().as_millis() as u64;
        let style_tree = crate::style::build_style_tree(&dom, &stylesheet, animation_time_ms);
        let layout_root = crate::layout::build_layout_tree(&style_tree, self.viewport_width as f32);
        let mut commands = Vec::new();
        crate::layout::collect_paint_commands(&layout_root, &mut commands);

        let actual_height = (layout_root.bounds.y + layout_root.bounds.height).ceil() as u32;
        let canvas_height = actual_height.max(400);
        let mut canvas = Canvas::new(self.viewport_width, canvas_height);
        if self.background.a > 0 {
            canvas.fill_rect(
                0.0,
                0.0,
                self.viewport_width as f32,
                canvas_height as f32,
                self.background,
            );
        }
        crate::paint::paint(&commands, &mut canvas, &self.rasterizer);

        self.page_canvas = canvas;
        self.layout_root = layout_root;
        self.needs_full_repaint = true;
    }

    /// Copy the visible slice of the page canvas into the window buffer.
    fn render_to_buffer(&mut self) {
        // Handle window resize.
        let (cw, ch) = self.window.get_size();
        if cw != self.window_width || ch != self.window_height {
            self.window_width = cw;
            self.window_height = ch;
            self.window_buffer.resize(cw * ch, 0xFFFFFFFF);
        }
        let w = self.window_width;
        let h = self.window_height;

        // Fill the address bar area with dark grey.
        for y in 0..ADDRESS_BAR_HEIGHT {
            for x in 0..w {
                self.window_buffer[y * w + x] = 0x2d2d2d;
            }
        }

        // Page content area: from ADDRESS_BAR_HEIGHT to h - STATUS_BAR_HEIGHT.
        let page_top = ADDRESS_BAR_HEIGHT;
        let page_bottom = h.saturating_sub(STATUS_BAR_HEIGHT);
        let visible_h = page_bottom.saturating_sub(page_top);
        let sy = self.scroll_y;

        let page_w = self.page_canvas.width as usize;
        let page_h = self.page_canvas.height as usize;

        for y in 0..visible_h {
            let py = y + sy;
            if py >= page_h {
                // Fill remaining with white.
                for x in 0..w {
                    self.window_buffer[(y + page_top) * w + x] = 0xFFFFFF;
                }
                continue;
            }
            for x in 0..w {
                if x >= page_w {
                    self.window_buffer[(y + page_top) * w + x] = 0xFFFFFF;
                    continue;
                }
                let src_idx = (py * page_w + x) * 4;
                let r = self.page_canvas.pixels[src_idx];
                let g = self.page_canvas.pixels[src_idx + 1];
                let b = self.page_canvas.pixels[src_idx + 2];
                self.window_buffer[(y + page_top) * w + x] =
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
    }

    /// Draw the address bar at the top of the window.
    fn draw_address_bar(&mut self) {
        let w = self.window_width;
        // Bar background already painted in render_to_buffer (dark grey).
        // Draw a lighter inner box for the URL field.
        let bar_x = 8usize;
        let bar_y = 4usize;
        let bar_w = w.saturating_sub(16);
        let bar_h = ADDRESS_BAR_HEIGHT - 8;
        for y in bar_y..(bar_y + bar_h) {
            for x in bar_x..(bar_x + bar_w) {
                self.window_buffer[y * w + x] = 0x404040;
            }
        }

        // Draw the URL text.
        let text = self.address_bar_text.clone();
        let text_color = if self.address_bar_focused {
            0xFFFFFF
        } else {
            0xCCCCCC
        };
        let mut cx = bar_x as i32 + 8;
        let cy = bar_y as i32 + 6;
        for c in text.chars() {
            if let Some(glyph) = self
                .rasterizer
                .rasterize(c, 14.0, "sans-serif", false, false)
            {
                let gw = glyph.width;
                let gh = glyph.height;
                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = glyph.mask[(gy * gw + gx) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        let px = cx + gx + glyph.offset_x;
                        let py = cy + gy + glyph.offset_y;
                        if px < 0 || px >= w as i32 || py < 0 || py >= self.window_height as i32 {
                            continue;
                        }
                        let val = (text_color >> 16) & 0xff;
                        let _v = (val * alpha as u32
                            + (self.window_buffer[(py as usize) * w + (px as usize)] >> 16 & 0xff)
                                * (255 - alpha as u32)
                                / 255)
                            & 0xff;
                        let cur = self.window_buffer[(py as usize) * w + (px as usize)];
                        let r = (((cur >> 16) & 0xff) * (255 - alpha as u32)
                            + (text_color >> 16 & 0xff) * alpha as u32)
                            / 255;
                        let g = (((cur >> 8) & 0xff) * (255 - alpha as u32)
                            + (text_color >> 8 & 0xff) * alpha as u32)
                            / 255;
                        let b = ((cur & 0xff) * (255 - alpha as u32)
                            + (text_color & 0xff) * alpha as u32)
                            / 255;
                        self.window_buffer[(py as usize) * w + (px as usize)] =
                            (r << 16) | (g << 8) | b;
                    }
                }
                cx += gw + glyph.offset_x;
                if cx > (bar_x + bar_w) as i32 - 8 {
                    break;
                }
            }
        }

        // Blinking caret if focused.
        if self.address_bar_focused && self.blink_on {
            let caret_x = (bar_x + 8 + crate::layout::measure_text(&text, 14.0) as usize) as i32;
            let cy0 = bar_y as i32 + 4;
            let cy1 = cy0 + bar_h as i32 - 8;
            for cy in cy0..cy1 {
                if cy >= 0 && cy < self.window_height as i32 && caret_x >= 0 && caret_x < w as i32 {
                    self.window_buffer[cy as usize * w + caret_x as usize] = 0xFFFFFF;
                }
            }
        }

        // Navigation hint text on the right.
        let hint = "F6: address  r: reload  Alt+←→: history  q: quit";
        let mut cx = w as i32 - 8;
        let mut hint_chars: Vec<char> = hint.chars().collect();
        hint_chars.reverse();
        for c in hint_chars {
            if let Some(glyph) = self
                .rasterizer
                .rasterize(c, 11.0, "sans-serif", false, false)
            {
                cx -= glyph.width + 1;
                if cx < (bar_x + bar_w + 16) as i32 {
                    break;
                }
                let gh = glyph.height;
                let gw = glyph.width;
                let cy_off = 9;
                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = glyph.mask[(gy * gw + gx) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        let px = cx + gx + glyph.offset_x;
                        let py = cy_off + gy + glyph.offset_y;
                        if px < 0 || px >= w as i32 || py < 0 || py >= ADDRESS_BAR_HEIGHT as i32 {
                            continue;
                        }
                        let cur = self.window_buffer[py as usize * w + px as usize];
                        let r = (((cur >> 16) & 0xff) * (255 - alpha as u32) + 0x88 * alpha as u32)
                            / 255;
                        let g = (((cur >> 8) & 0xff) * (255 - alpha as u32) + 0x88 * alpha as u32)
                            / 255;
                        let b = ((cur & 0xff) * (255 - alpha as u32) + 0x88 * alpha as u32) / 255;
                        self.window_buffer[py as usize * w + px as usize] =
                            (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }

    /// Draw a focus ring around the focused element.
    fn draw_focus_indicator(&mut self) {
        if let Some(focused_id) = self.focused_element {
            if let Some(elem) = self
                .interactive_elements
                .iter()
                .find(|e| e.id == focused_id)
            {
                let b = elem.bounds;
                let sy = self.scroll_y as f32;
                let y_offset = ADDRESS_BAR_HEIGHT as f32;
                let screen_y = b.y - sy + y_offset;
                if screen_y + b.height < 0.0 || screen_y > self.window_height as f32 {
                    return;
                }
                let ring_color: u32 = 0x3b82f6;
                let x0 = (b.x as i32).max(0) as usize;
                let y0 = (screen_y as i32).max(0) as usize;
                let x1 = ((b.x + b.width) as i32).min(self.window_width as i32) as usize;
                let y1 = ((screen_y + b.height) as i32).min(self.window_height as i32) as usize;
                for y in y0..y1 {
                    for dx in 0..2 {
                        let px = x0 + dx;
                        if px < self.window_width {
                            self.window_buffer[y * self.window_width + px] = ring_color;
                        }
                        let px = x1.saturating_sub(1 + dx);
                        if px < self.window_width {
                            self.window_buffer[y * self.window_width + px] = ring_color;
                        }
                    }
                }
                for x in x0..x1 {
                    for dy in 0..2 {
                        let py = y0 + dy;
                        if py < self.window_height {
                            self.window_buffer[py * self.window_width + x] = ring_color;
                        }
                        let py = y1.saturating_sub(1 + dy);
                        if py < self.window_height {
                            self.window_buffer[py * self.window_width + x] = ring_color;
                        }
                    }
                }

                if elem.kind == InteractiveKind::Input && self.blink_on {
                    let text = &self.input_text;
                    let caret_x = b.x + 8.0 + crate::layout::measure_text(text, 16.0);
                    let caret_y = b.y + 6.0 - sy + y_offset;
                    let cx = caret_x as i32;
                    let cy0 = caret_y as i32;
                    let cy1 = (caret_y + 20.0) as i32;
                    if cx >= 0 && cx < self.window_width as i32 {
                        for cy in cy0..cy1 {
                            if cy >= 0 && cy < self.window_height as i32 {
                                self.window_buffer[cy as usize * self.window_width + cx as usize] =
                                    0x000000;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draw the status bar at the bottom of the window.
    fn draw_status_bar(&mut self) {
        let w = self.window_width;
        let h = self.window_height;
        let bar_y = h.saturating_sub(STATUS_BAR_HEIGHT);

        for y in bar_y..h {
            for x in 0..w {
                self.window_buffer[y * w + x] = 0x1a1a1a;
            }
        }

        let text = self.status.clone();
        let text = if text.len() > 120 {
            text[..120].to_string()
        } else {
            text
        };
        let mut cx = 8i32;
        let cy = (bar_y as i32) + 4;
        for c in text.chars() {
            if let Some(glyph) = self
                .rasterizer
                .rasterize(c, 14.0, "sans-serif", false, false)
            {
                let gw = glyph.width;
                let gh = glyph.height;
                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = glyph.mask[(gy * gw + gx) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        let px = cx + gx + glyph.offset_x;
                        let py = cy + gy + glyph.offset_y;
                        if px < 0 || px >= w as i32 || py < 0 || py >= h as i32 {
                            continue;
                        }
                        let val = 200u32 + ((alpha as u32) * 55 / 255);
                        self.window_buffer[(py as usize) * w + (px as usize)] =
                            (val << 16) | (val << 8) | val;
                    }
                }
                cx += gw + glyph.offset_x;
                if cx > w as i32 - 16 {
                    break;
                }
            }
        }
    }

    fn check_hover(&mut self, mx: isize, my: isize) {
        if mx < 0 || my < 0 {
            return;
        }
        // Adjust for address bar offset.
        let abs_y = (my - ADDRESS_BAR_HEIGHT as isize) as f32 + self.scroll_y as f32;
        if abs_y < 0.0 {
            return;
        }
        let new_hover = self.find_link_at(mx as f32, abs_y);
        if new_hover.is_some() {
            self.status = format!("→ {}", new_hover.unwrap());
        } else {
            self.update_status();
        }
    }

    fn handle_click(&mut self, mx: isize, my: isize) {
        if mx < 0 || my < 0 {
            return;
        }

        // Click in address bar?
        if (my as usize) < ADDRESS_BAR_HEIGHT {
            self.address_bar_focused = true;
            self.focused_element = None;
            return;
        }

        // Click in page area.
        self.address_bar_focused = false;
        let abs_y = (my - ADDRESS_BAR_HEIGHT as isize) as f32 + self.scroll_y as f32;

        // Find the clicked element (clone to avoid borrow conflict).
        let mut clicked: Option<(
            InteractiveKind,
            Option<String>,
            usize,
            String,
            std::collections::HashMap<String, String>,
        )> = None;
        for elem in &self.interactive_elements {
            let b = elem.bounds;
            if mx as f32 >= b.x
                && mx as f32 <= b.x + b.width
                && abs_y >= b.y
                && abs_y <= b.y + b.height
            {
                clicked = Some((
                    elem.kind.clone(),
                    elem.href.clone(),
                    elem.id,
                    elem.initial_value.clone(),
                    elem.event_handlers.clone(),
                ));
                break;
            }
        }

        if let Some((kind, href, id, value, handlers)) = clicked {
            // Execute onclick handler if present.
            if let Some(onclick) = handlers.get("onclick") {
                eprintln!("[falco:tjs] executing onclick: {}", onclick);
                let mut tjs = crate::tjs::TjsContext::new();
                if let Err(e) = tjs.execute(onclick) {
                    eprintln!("[falco:tjs] onclick error: {}", e);
                }
            }

            match kind {
                InteractiveKind::Link => {
                    if let Some(h) = href {
                        self.navigate(&h);
                    }
                }
                InteractiveKind::Input | InteractiveKind::Textarea => {
                    self.focused_element = Some(id);
                    self.input_text = value;
                    if handlers.is_empty() {
                        self.status = "Focused input — type to enter text".to_string();
                    }
                }
                InteractiveKind::Button => {
                    if !handlers.contains_key("onclick") {
                        self.status = format!("Button clicked: {:?}", value);
                    }
                    eprintln!("[falco] button clicked");
                }
                InteractiveKind::Checkbox => {
                    self.status = "Checkbox toggled".to_string();
                    eprintln!("[falco] checkbox toggled");
                }
            }
        } else {
            self.focused_element = None;
            self.input_text.clear();
            self.update_status();
        }
    }

    fn find_link_at(&self, x: f32, y: f32) -> Option<String> {
        for elem in &self.interactive_elements {
            if elem.kind == InteractiveKind::Link {
                let b = elem.bounds;
                if x >= b.x && x <= b.x + b.width && y >= b.y && y <= b.y + b.height {
                    return elem.href.clone();
                }
            }
        }
        None
    }

    fn cycle_focus(&mut self, reverse: bool) {
        if self.interactive_elements.is_empty() {
            return;
        }
        let ids: Vec<usize> = self.interactive_elements.iter().map(|e| e.id).collect();
        if let Some(current) = self.focused_element {
            let idx = ids.iter().position(|&id| id == current);
            if let Some(i) = idx {
                let new_idx = if reverse {
                    (i + ids.len() - 1) % ids.len()
                } else {
                    (i + 1) % ids.len()
                };
                self.focused_element = Some(ids[new_idx]);
            } else {
                self.focused_element = Some(ids[0]);
            }
        } else {
            self.focused_element = Some(ids[0]);
        }
        if let Some(fid) = self.focused_element {
            if let Some(elem) = self.interactive_elements.iter().find(|e| e.id == fid) {
                self.input_text = elem.initial_value.clone();
            }
        }
        self.status = format!("Focused element {}", self.focused_element.unwrap_or(0));
    }
}

/// Scan a layout tree for interactive elements.
struct ElementScanner<'a> {
    elements: &'a mut Vec<InteractiveElement>,
    next_id: usize,
}

impl<'a> ElementScanner<'a> {
    fn scan(&mut self, box_node: &LayoutBox) {
        if let Some(tag) = &box_node.tag {
            // Extract event handlers from attrs.
            let handlers = crate::js_runner::extract_event_handlers(&box_node.attrs);
            let handlers_map: std::collections::HashMap<String, String> =
                handlers.into_iter().collect();

            match tag.as_str() {
                "a" if box_node.attrs.contains_key("href") => {
                    let href = box_node.attrs.get("href").cloned();
                    let id = self.next_id;
                    self.next_id += 1;
                    self.elements.push(InteractiveElement {
                        id,
                        bounds: box_node.bounds,
                        kind: InteractiveKind::Link,
                        href,
                        initial_value: String::new(),
                        event_handlers: handlers_map,
                    });
                }
                "input" => {
                    let input_type = box_node
                        .attrs
                        .get("type")
                        .map(|s| s.as_str())
                        .unwrap_or("text");
                    let kind = match input_type {
                        "submit" | "button" | "reset" => InteractiveKind::Button,
                        "checkbox" | "radio" => InteractiveKind::Checkbox,
                        _ => InteractiveKind::Input,
                    };
                    let id = self.next_id;
                    self.next_id += 1;
                    let value = box_node.attrs.get("value").cloned().unwrap_or_default();
                    self.elements.push(InteractiveElement {
                        id,
                        bounds: box_node.bounds,
                        kind,
                        href: None,
                        initial_value: value,
                        event_handlers: handlers_map,
                    });
                }
                "button" => {
                    let id = self.next_id;
                    self.next_id += 1;
                    let mut text = String::new();
                    collect_text(box_node, &mut text);
                    self.elements.push(InteractiveElement {
                        id,
                        bounds: box_node.bounds,
                        kind: InteractiveKind::Button,
                        href: None,
                        initial_value: text,
                        event_handlers: handlers_map,
                    });
                }
                "textarea" => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.elements.push(InteractiveElement {
                        id,
                        bounds: box_node.bounds,
                        kind: InteractiveKind::Textarea,
                        href: None,
                        initial_value: String::new(),
                        event_handlers: handlers_map,
                    });
                }
                _ => {
                    // Even non-interactive elements can have onclick handlers.
                    if handlers_map.contains_key("onclick") {
                        let id = self.next_id;
                        self.next_id += 1;
                        self.elements.push(InteractiveElement {
                            id,
                            bounds: box_node.bounds,
                            kind: InteractiveKind::Button,
                            href: None,
                            initial_value: String::new(),
                            event_handlers: handlers_map,
                        });
                    }
                }
            }
        }
        // Recurse.
        let mut elements = std::mem::take(self.elements);
        let mut scanner = ElementScanner {
            elements: &mut elements,
            next_id: self.next_id,
        };
        match &box_node.content {
            BoxContent::Block(children) | BoxContent::Flex(children) => {
                for c in children {
                    scanner.scan(c);
                }
            }
            BoxContent::Inline(items) => {
                for item in items {
                    if let InlineItem::Text(child) = item {
                        scanner.scan(child);
                    }
                }
            }
            _ => {}
        }
        self.next_id = scanner.next_id;
        *self.elements = elements;
    }
}

fn patch_input_value(box_node: &mut LayoutBox, _target_id: usize, new_value: &str) {
    if box_node.tag.as_deref() == Some("input") {
        let input_type = box_node
            .attrs
            .get("type")
            .map(|s| s.as_str())
            .unwrap_or("text");
        if input_type == "text" || input_type == "password" || input_type == "email" {
            box_node
                .attrs
                .insert("value".to_string(), new_value.to_string());
            return;
        }
    }
    match &mut box_node.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                patch_input_value(c, _target_id, new_value);
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    patch_input_value(child, _target_id, new_value);
                }
            }
        }
        _ => {}
    }
}

fn collect_text(box_node: &LayoutBox, out: &mut String) {
    match &box_node.content {
        BoxContent::Text { text, .. } => out.push_str(text),
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                collect_text(c, out);
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    collect_text(child, out);
                }
            }
        }
        _ => {}
    }
}

fn key_to_char(key: Key, shift: bool) -> Option<char> {
    use Key::*;
    let c = match key {
        A => 'a',
        B => 'b',
        C => 'c',
        D => 'd',
        E => 'e',
        F => 'f',
        G => 'g',
        H => 'h',
        I => 'i',
        J => 'j',
        K => 'k',
        L => 'l',
        M => 'm',
        N => 'n',
        O => 'o',
        P => 'p',
        Q => 'q',
        R => 'r',
        S => 's',
        T => 't',
        U => 'u',
        V => 'v',
        W => 'w',
        X => 'x',
        Y => 'y',
        Z => 'z',
        Key0 => '0',
        Key1 => '1',
        Key2 => '2',
        Key3 => '3',
        Key4 => '4',
        Key5 => '5',
        Key6 => '6',
        Key7 => '7',
        Key8 => '8',
        Key9 => '9',
        Space => ' ',
        Minus => '-',
        Equal => '=',
        LeftBracket => '[',
        RightBracket => ']',
        Semicolon => ';',
        Apostrophe => '\'',
        Comma => ',',
        Period => '.',
        Slash => '/',
        Backslash => '\\',
        _Grave => '`',
        _ => return None,
    };
    if shift && c.is_ascii_alphabetic() {
        Some(c.to_ascii_uppercase())
    } else if shift {
        Some(match c {
            '1' => '!',
            '2' => '@',
            '3' => '#',
            '4' => '$',
            '5' => '%',
            '6' => '^',
            '7' => '&',
            '8' => '*',
            '9' => '(',
            '0' => ')',
            '-' => '_',
            '=' => '+',
            '[' => '{',
            ']' => '}',
            ';' => ':',
            '\'' => '"',
            ',' => '<',
            '.' => '>',
            '/' => '?',
            '\\' => '|',
            '`' => '~',
            other => other,
        })
    } else {
        Some(c)
    }
}

/// Extract the contents of all `<style>...</style>` tags from an HTML source.
fn extract_style_tags(html: &str) -> String {
    let mut out = String::new();
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(open) = lower[search_from..].find("<style") {
        let open_abs = search_from + open;
        let Some(close_rel) = lower[open_abs..].find('>') else {
            break;
        };
        let content_start = open_abs + close_rel + 1;
        let Some(end_rel) = lower[content_start..].find("</style>") else {
            break;
        };
        let content_end = content_start + end_rel;
        out.push_str(&html[content_start..content_end]);
        out.push('\n');
        search_from = content_end + 8;
    }
    out
}

/// Strip all <script>...</script> tags from HTML.
fn strip_scripts_window(html: &str) -> String {
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
/// Does NOT override colors or backgrounds (let the site's own CSS work).
fn inject_fallback_css_window(html: &str) -> String {
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

/// Extract <style> tag contents.
fn extract_style_tags_window(html: &str) -> String {
    extract_style_tags(html)
}
