//! Image loading — fetches images from HTTP URLs or decodes data: URLs,
//! caches them, and provides RGBA pixel data to the painter.
//!
//! Supported formats: PNG, JPEG, GIF (first frame), BMP.
//! Supported sources: `http://`, `https://`, `data:image/png;base64,...`,
//! and local file paths (relative to the page's base URL).

use crate::css::Color;
use crate::paint::Canvas;
use std::collections::HashMap;
use std::sync::Mutex;

/// A loaded image — RGBA pixel data + dimensions.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA, row-major
}

/// Global image cache — keyed by URL. Prevents re-fetching the same image.
static IMAGE_CACHE: Mutex<Option<HashMap<String, Image>>> = Mutex::new(None);

/// The current page's base URL — set before rendering so that relative
/// image URLs can be resolved. Set via `set_base_url()`.
static BASE_URL: Mutex<String> = Mutex::new(String::new());

/// Set the current page's base URL (called before rendering).
pub fn set_base_url(url: &str) {
    *BASE_URL.lock().unwrap() = url.to_string();
}

/// Get the current page's base URL.
pub fn get_base_url() -> String {
    BASE_URL.lock().unwrap().clone()
}

/// Load an image from a URL or data: URI. Uses the global cache.
/// Returns `None` if the image cannot be loaded or decoded.
pub fn load_image(src: &str, base_url: &str) -> Option<Image> {
    // Resolve relative URLs.
    let resolved = resolve_url(src, base_url);

    // Check cache.
    {
        let cache = IMAGE_CACHE.lock().unwrap();
        if let Some(ref cache) = *cache {
            if let Some(img) = cache.get(&resolved) {
                return Some(img.clone());
            }
        }
    }

    // Load the image.
    let image = if resolved.starts_with("data:") {
        load_data_url(&resolved)
    } else if resolved.starts_with("http://") || resolved.starts_with("https://") {
        load_http_url(&resolved)
    } else {
        // Local file path.
        load_local_file(&resolved)
    };

    // Cache the result (including None, to avoid retrying broken URLs).
    if let Some(ref img) = image {
        let mut cache = IMAGE_CACHE.lock().unwrap();
        if cache.is_none() {
            *cache = Some(HashMap::new());
        }
        cache.as_mut().unwrap().insert(resolved, img.clone());
    }

    image
}

/// Resolve a URL relative to a base URL.
pub fn resolve_url_pub(src: &str, base: &str) -> String {
    resolve_url(src, base)
}

/// Resolve a URL relative to a base URL.
fn resolve_url(src: &str, base: &str) -> String {
    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
        return src.to_string();
    }
    if src.starts_with("/") {
        // Absolute path — resolve against base origin.
        if let Some(scheme_end) = base.find("://") {
            if let Some(path_start) = base[scheme_end + 3..].find('/') {
                return format!("{}{}", &base[..scheme_end + 3 + path_start], src);
            }
            return format!("{}{}", base, src);
        }
        return src.to_string();
    }
    if src.starts_with("#") {
        return base.to_string();
    }
    // Relative path — append to base directory.
    if base.is_empty() {
        return src.to_string();
    }
    let last_slash = base.rfind('/').unwrap_or(base.len().saturating_sub(1));
    format!("{}{}", &base[..last_slash + 1], src)
}

/// Decode a data: URL (e.g. `data:image/png;base64,iVBORw0KGgo...`).
fn load_data_url(url: &str) -> Option<Image> {
    let rest = url.strip_prefix("data:")?;
    let comma = rest.find(',')?;
    let header = &rest[..comma];
    let data = &rest[comma + 1..];

    // Parse header: "image/png;base64" or "image/png" or ";base64"
    let is_base64 = header.contains("base64");
    let _mime = header.split(';').next().unwrap_or("image/png");

    let bytes = if is_base64 {
        use base64::{engine::general_purpose, Engine as _};
        general_purpose::STANDARD.decode(data).ok()?
    } else {
        // URL-encoded data — just use as-is (rare in practice).
        data.as_bytes().to_vec()
    };

    decode_image_bytes(&bytes)
}

/// Fetch an image from an HTTP/HTTPS URL.
fn load_http_url(url: &str) -> Option<Image> {
    let bytes = crate::net::fetch_bytes(url).ok()?;
    decode_image_bytes(&bytes)
}

/// Load an image from a local file path.
fn load_local_file(path: &str) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    decode_image_bytes(&bytes)
}

/// Decode raw image bytes (PNG, JPEG, GIF, BMP) into an Image.
fn decode_image_bytes(bytes: &[u8]) -> Option<Image> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(Image {
        width: w,
        height: h,
        pixels: rgba.into_raw(),
    })
}

/// Draw an image onto a canvas at the given position and size.
/// If the image dimensions don't match, it's scaled using nearest-neighbor.
pub fn draw_image_to_canvas(canvas: &mut Canvas, img: &Image, x: f32, y: f32, w: f32, h: f32) {
    let x0 = x.max(0.0).floor() as i32;
    let y0 = y.max(0.0).floor() as i32;
    let x1 = ((x + w).min(canvas.width as f32)).ceil() as i32;
    let y1 = ((y + h).min(canvas.height as f32)).ceil() as i32;

    let scale_x = img.width as f32 / w.max(1.0);
    let scale_y = img.height as f32 / h.max(1.0);

    for py in y0..y1 {
        for px in x0..x1 {
            if px < 0 || px >= canvas.width as i32 || py < 0 || py >= canvas.height as i32 {
                continue;
            }
            // Map canvas pixel to image pixel (nearest-neighbor).
            let src_x = ((px as f32 - x) * scale_x) as u32;
            let src_y = ((py as f32 - y) * scale_y) as u32;
            if src_x >= img.width || src_y >= img.height {
                continue;
            }
            let src_idx = ((src_y * img.width + src_x) * 4) as usize;
            let r = img.pixels[src_idx];
            let g = img.pixels[src_idx + 1];
            let b = img.pixels[src_idx + 2];
            let a = img.pixels[src_idx + 3];
            let dst_idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
            blend(
                &mut canvas.pixels[dst_idx..dst_idx + 4],
                Color::rgba(r, g, b, a),
            );
        }
    }
}

fn blend(dst: &mut [u8], src: Color) {
    if src.a == 0 {
        return;
    }
    if src.a == 255 {
        dst[0] = src.r;
        dst[1] = src.g;
        dst[2] = src.b;
        dst[3] = 255;
        return;
    }
    let sa = src.a as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a == 0.0 {
        return;
    }
    dst[0] = ((src.r as f32 * sa + dst[0] as f32 * da * (1.0 - sa)) / out_a) as u8;
    dst[1] = ((src.g as f32 * sa + dst[1] as f32 * da * (1.0 - sa)) / out_a) as u8;
    dst[2] = ((src.b as f32 * sa + dst[2] as f32 * da * (1.0 - sa)) / out_a) as u8;
    dst[3] = (out_a * 255.0) as u8;
}

/// Clear the global image cache (called on page navigation).
pub fn clear_cache() {
    let mut cache = IMAGE_CACHE.lock().unwrap();
    *cache = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_urls() {
        assert_eq!(
            resolve_url("/foo", "https://example.com/bar"),
            "https://example.com/foo"
        );
        assert_eq!(
            resolve_url("img.png", "https://example.com/page.html"),
            "https://example.com/img.png"
        );
        assert_eq!(
            resolve_url("https://other.com/x", "https://example.com"),
            "https://other.com/x"
        );
        assert_eq!(
            resolve_url("#anchor", "https://example.com"),
            "https://example.com"
        );
    }

    #[test]
    fn decodes_data_url() {
        // 1x1 PNG as base64.
        let url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        let img = load_data_url(url);
        assert!(img.is_some());
        let img = img.unwrap();
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
        assert_eq!(img.pixels.len(), 4); // RGBA
    }
}
