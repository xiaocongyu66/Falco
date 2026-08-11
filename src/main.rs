//! Falco CLI — render an HTML file or URL to a PNG, or open an interactive window.

use pico_args::Arguments;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::ExitCode;

/// Global storage for standalone SVG content. When a standalone SVG file
/// is loaded, the original XML is stored here so that `paint_inline_svg`
/// in the layout module can access it without going through the HTML parser
/// (which would mangle deeply nested <g> elements).

fn main() -> ExitCode {
    let mut args = Arguments::from_env();
    let usage = r#"falco — a tiny browser engine

USAGE
  falco <input> [OPTIONS]
  falco --server [OPTIONS]
  falco --js-run <file.js>

INPUT
  A URL (http://...) or a path to a local .html file.

OPTIONS
  --css <path>       External CSS file (merged with <style> tags in HTML).
  --out <path>       Output PNG path. Default: falco.png
  --width <px>       Viewport width. Default: 1200
  --height <px>      Viewport height (canvas grows if content is taller). Default: 800
  --bg <hex>         Background color (0xRRGGBBAA). Default: 0xFFFFFFFF
  --window           Open a live interactive window instead of writing PNG.
  --sandbox          Enable process sandbox (seccomp on Linux, Job Object on Windows).
  --server           Start HTTP server mode (port 8899). Open falco_ui.html in a browser.
  --port <n>         Server port. Default: 8899
  -h, --help         Show this help
  -V, --version      Print version

EXAMPLES
  falco page.html --out page.png
  falco https://example.com --width 800 --out example.png
  falco page.html --window
  falco --server
"#;

    if args.contains(["-h", "--help"]) {
        print!("{usage}");
        return ExitCode::SUCCESS;
    }
    if args.contains(["-V", "--version"]) {
        println!("falco {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    // Server mode: start a simple HTTP server that renders URLs to PNG.
    if args.contains("--server") {
        let port: u16 = args
            .opt_value_from_str("--port")
            .ok()
            .flatten()
            .unwrap_or(8899);
        return run_server(port);
    }

    // Parse named args FIRST, then free args.
    let css_path: Option<String> = args.opt_value_from_str("--css").ok().flatten();
    let out_path: String = args
        .opt_value_from_str("--out")
        .ok()
        .flatten()
        .unwrap_or_else(|| "falco.png".to_string());
    let width: u32 = args
        .opt_value_from_str("--width")
        .ok()
        .flatten()
        .unwrap_or(1200);
    let height: u32 = args
        .opt_value_from_str("--height")
        .ok()
        .flatten()
        .unwrap_or(800);
    let bg: u32 = args
        .opt_value_from_str("--bg")
        .ok()
        .flatten()
        .unwrap_or(0xFFFFFFFF);
    let want_window: bool = args.contains("--window");
    let want_sandbox: bool = args.contains("--sandbox");
    let js_run: Option<String> = args.opt_value_from_str("--js-run").ok().flatten();
    let bg = if bg <= 0xFFFFFF { bg << 8 | 0xFF } else { bg };
    let input: Option<String> = args.opt_free_from_str().ok().flatten();

    // Apply sandbox if requested (--sandbox flag).
    // On Linux: seccomp-bpf filter that restricts syscalls.
    // On Windows: Job Object with process limits.
    // This is opt-in to avoid breaking dev workflows.
    if want_sandbox {
        #[cfg(feature = "sandbox")]
        {
            if let Err(e) = falco::security::sandbox::apply() {
                eprintln!("[falco:sandbox] failed to apply sandbox: {}", e);
                eprintln!("[falco:sandbox] continuing WITHOUT sandbox — this is unsafe for untrusted content");
            } else {
                eprintln!("[falco:sandbox] sandbox active");
            }
        }
        #[cfg(not(feature = "sandbox"))]
        {
            eprintln!("[falco:sandbox] sandbox feature not enabled — rebuild with: cargo build --features sandbox");
        }
    }

    // JS-run mode: execute a JS file and exit.
    if let Some(js_path) = js_run {
        match falco::js_runner::run_js_file(&js_path) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: JS execution failed: {e}");
                return ExitCode::from(4);
            }
        }
    }

    let Some(input) = input else {
        eprintln!("error: missing input file or URL\n\n{usage}");
        return ExitCode::from(1);
    };

    // Fetch HTML.
    // Strip URL fragment (#...) — it's client-side only, not sent to server.
    let input_clean = if let Some(hash_pos) = input.find('#') {
        eprintln!("[falco] stripping URL fragment: {}", &input[hash_pos..]);
        input[..hash_pos].to_string()
    } else {
        input.clone()
    };
    let html_src = if falco::net::is_url(&input_clean) {
        match falco::net::fetch(&input_clean) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to fetch {input}: {e}");
                return ExitCode::from(2);
            }
        }
    } else if input_clean.ends_with(".svgz") {
        // .svgz = gzip-compressed SVG. Read raw bytes and decompress.
        match std::fs::read(&input_clean) {
            Ok(bytes) => {
                if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
                    eprintln!("[falco] decompressing .svgz file ({} bytes)", bytes.len());
                    match decompress_gzip_local(&bytes) {
                        Ok(decompressed) => String::from_utf8_lossy(&decompressed).to_string(),
                        Err(e) => {
                            eprintln!("error: failed to decompress {input_clean}: {e}");
                            return ExitCode::from(2);
                        }
                    }
                } else {
                    String::from_utf8_lossy(&bytes).to_string()
                }
            }
            Err(e) => {
                eprintln!("error: failed to read {input_clean}: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        match falco::net::read_file(&input_clean) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to read {input}: {e}");
                return ExitCode::from(2);
            }
        }
    };

    // Detect standalone SVG content and wrap it in HTML so the inline
    // SVG renderer can handle it. Standalone SVG files start with
    // <?xml or <svg and don't have <html> or <body> tags.
    let html_src = if html_src.trim_start().starts_with("<?xml")
        || html_src.trim_start().to_lowercase().starts_with("<svg")
    {
        eprintln!("[falco] detected standalone SVG, wrapping in HTML");
        // Strip XML declaration — it confuses the HTML parser.
        let svg_content = if html_src.trim_start().starts_with("<?xml") {
            html_src
                .split("?>")
                .nth(1)
                .unwrap_or(&html_src)
                .trim_start()
        } else {
            html_src.trim_start()
        };
        // Store the original SVG XML in a global static so paint_inline_svg
        // can access it. Using a global avoids putting 3MB of SVG content
        // into an HTML attribute (which the HTML parser can't handle).
        falco::set_standalone_svg(svg_content.to_string());
        format!(
            "<!DOCTYPE html>\n<html><head><style>\n\
             body {{ margin: 0; padding: 0; background: white; }}\n\
             svg {{ display: block; }}\n\
             </style></head><body>\n\
             <svg data-falco-standalone=\"true\"></svg>\n\
             </body></html>"
        )
    } else {
        html_src
    };
    let mut css_src = extract_style_tags(&html_src);
    if let Some(path) = css_path {
        match std::fs::read_to_string(&path) {
            Ok(s) => css_src.push_str(&s),
            Err(e) => {
                eprintln!("warning: failed to read CSS file {path}: {e}");
            }
        }
    }

    if want_window {
        // Open interactive window. If the window can't be created (headless
        // environment, no DISPLAY), fall back to PNG output with a clear message.
        match open_window(&input, &html_src, &css_src, width, bg) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("Failed to create window")
                    || msg.contains("DISPLAY")
                    || msg.contains("Wayland")
                {
                    eprintln!("warning: cannot open interactive window (no display available)");
                    eprintln!("         falling back to PNG output. Run on a desktop machine to use --window.");
                    let opts = falco::RenderOptions {
                        width,
                        height,
                        background: bg,
                        animation_time_ms: 0,
                    };
                    match falco::render_to_png(&html_src, &css_src, opts, &out_path) {
                        Ok(()) => {
                            eprintln!("Rendered to {out_path}");
                            ExitCode::SUCCESS
                        }
                        Err(e2) => {
                            eprintln!("error: render failed: {e2:#}");
                            ExitCode::from(3)
                        }
                    }
                } else {
                    eprintln!("error: window failed: {msg}");
                    ExitCode::from(3)
                }
            }
        }
    } else {
        // PNG output mode.
        let opts = falco::RenderOptions {
            width,
            height,
            background: bg,
            animation_time_ms: 0,
        };
        match falco::render_to_png(&html_src, &css_src, opts, &out_path) {
            Ok(()) => {
                eprintln!("Rendered to {out_path}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: render failed: {e:#}");
                ExitCode::from(3)
            }
        }
    }
}

/// Open the interactive browser window.
fn open_window(
    url: &str,
    html_src: &str,
    css_src: &str,
    width: u32,
    bg: u32,
) -> anyhow::Result<()> {
    let bg_color = falco::css::Color::rgba(
        ((bg >> 24) & 0xff) as u8,
        ((bg >> 16) & 0xff) as u8,
        ((bg >> 8) & 0xff) as u8,
        (bg & 0xff) as u8,
    );
    let title = format!("Falco — {}", url);
    falco::window::Browser::open(
        &title,
        html_src.to_string(),
        css_src.to_string(),
        url.to_string(),
        width,
        bg_color,
    )?;
    Ok(())
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

/// Run Falco as an HTTP server. Renders URLs to PNG on demand.
/// Usage: falco --server
/// Then open falco_ui.html in any browser.
fn run_server(port: u16) -> ExitCode {
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: failed to bind port {}: {}", port, e);
            return ExitCode::from(5);
        }
    };
    eprintln!("Falco server running on http://localhost:{}", port);
    eprintln!("Open falco_ui.html in your browser to use the UI.");
    eprintln!("Press Ctrl+C to stop.");

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Read the HTTP request.
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the request line.
        let first_line = request.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let method = parts[0];
        let path = parts[1];

        // Handle CORS preflight.
        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }

        // Handle /render?url=...
        if path.starts_with("/render") {
            // Extract URL from query string.
            let url = path
                .split("url=")
                .nth(1)
                .map(|s| s.split('&').next().unwrap_or(""))
                .map(url_decode)
                .unwrap_or_default();

            if url.is_empty() {
                let resp = "HTTP/1.1 400 Bad Request\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\n\r\nMissing url parameter";
                let _ = stream.write_all(resp.as_bytes());
                continue;
            }

            eprintln!("[server] rendering: {}", url);

            // Fetch HTML.
            let html_src = if falco::net::is_url(&url) {
                match falco::net::fetch(&url) {
                    Ok(s) => s,
                    Err(e) => {
                        let resp = format!("HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\n\r\nFetch error: {}", e);
                        let _ = stream.write_all(resp.as_bytes());
                        continue;
                    }
                }
            } else {
                match std::fs::read_to_string(&url) {
                    Ok(s) => s,
                    Err(e) => {
                        let resp = format!("HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\n\r\nFile error: {}", e);
                        let _ = stream.write_all(resp.as_bytes());
                        continue;
                    }
                }
            };

            // Extract CSS from <style> tags.
            let css_src = extract_style_tags(&html_src);

            // Render.
            let opts = falco::RenderOptions {
                width: 1280,
                height: 800,
                background: 0xFFFFFFFF,
                animation_time_ms: 0,
            };
            match falco::render_with_base_url(&html_src, &css_src, opts, &url) {
                Ok((canvas, _)) => {
                    let png_bytes = falco::png::encode(canvas.width, canvas.height, &canvas.pixels);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: image/png\r\nContent-Length: {}\r\n\r\n",
                        png_bytes.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.write_all(&png_bytes);
                    eprintln!("[server] rendered: {} bytes", png_bytes.len());
                }
                Err(e) => {
                    let resp = format!("HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\n\r\nRender error: {:#}", e);
                    let _ = stream.write_all(resp.as_bytes());
                }
            }
        } else if path == "/" || path == "/index.html" {
            // Serve a minimal built-in UI (no external file required).
            let ui = FALCO_UI_HTML;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                ui.len(),
                ui
            );
            let _ = stream.write_all(resp.as_bytes());
        } else {
            let resp = "HTTP/1.1 404 Not Found\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\n\r\nNot found";
            let _ = stream.write_all(resp.as_bytes());
        }
    }

    ExitCode::SUCCESS
}

/// Decompress gzip-compressed bytes (for .svgz files).
fn decompress_gzip_local(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(bytes);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Simple URL decoder (handles %XX encoding).
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                let hex = format!("{}{}", h1, h2);
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

/// Built-in HTML for the `--server` mode UI (served at `/`).
///
/// This is intentionally minimal — a single text input + an `<img>` that
/// fetches from `/render?url=...`. Kept inline so the binary has no
/// external file dependency.
const FALCO_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Falco — render server</title>
  <style>
    :root { color-scheme: dark; }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
      background: #0d1117;
      color: #c9d1d9;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      padding: 32px 16px;
    }
    h1 { font-size: 22px; font-weight: 600; margin: 0 0 6px; }
    p  { color: #8b949e; font-size: 13px; margin: 0 0 24px; }
    form {
      width: min(720px, 100%);
      display: flex;
      gap: 8px;
      margin-bottom: 24px;
    }
    input[type="url"] {
      flex: 1;
      padding: 10px 14px;
      background: #161b22;
      border: 1px solid #30363d;
      border-radius: 6px;
      color: #c9d1d9;
      font-size: 14px;
      outline: none;
    }
    input[type="url"]:focus { border-color: #58a6ff; }
    button {
      padding: 10px 22px;
      background: #238636;
      color: #fff;
      border: none;
      border-radius: 6px;
      font-weight: 600;
      font-size: 14px;
      cursor: pointer;
    }
    button:hover { background: #2ea043; }
    .out {
      width: min(1200px, 100%);
      max-width: 100%;
      background: #161b22;
      border: 1px solid #30363d;
      border-radius: 8px;
      overflow: hidden;
    }
    img { display: block; max-width: 100%; height: auto; }
    .empty { padding: 64px; text-align: center; color: #6e7681; }
    .author {
      margin-top: 32px;
      font-size: 12px;
      color: #6e7681;
    }
    a { color: #58a6ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <h1>Falco render server</h1>
  <p>Enter a URL — Falco will render it server-side and stream back the PNG.</p>

  <form id="f">
    <input id="url" type="url" placeholder="https://example.com" value="https://example.com" autofocus>
    <button type="submit">Render</button>
  </form>

  <div class="out" id="out">
    <div class="empty">No render yet.</div>
  </div>

  <div class="author">
    Logo by <strong>Sanya</strong> —
    <a href="https://t.me/SanyochekDev" target="_blank" rel="noopener">t.me/SanyochekDev</a>
  </div>

  <script>
    const form = document.getElementById('f');
    const url  = document.getElementById('url');
    const out  = document.getElementById('out');
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const u = encodeURIComponent(url.value.trim());
      if (!u) return;
      out.innerHTML = '<div class="empty">Rendering…</div>';
      const img = new Image();
      img.onload = () => { out.replaceChildren(img); };
      img.onerror = () => { out.innerHTML = '<div class="empty">Render failed.</div>'; };
      img.src = '/render?url=' + u;
    });
  </script>
</body>
</html>"#;
