//! Paint — render paint commands to an RGBA canvas.

pub mod decorations;
pub mod text_shaping;
pub mod harfbuzz_shaping;
#[cfg(feature = "real-gpu")]
pub mod gpu_render;

use crate::css::Color;
use crate::style::BorderRadius;

/// A paint command emitted by the layout engine.
#[derive(Debug, Clone)]
pub enum PaintCommand {
    /// Filled rectangle (legacy, kept for compat).
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    /// Filled rectangle with optional rounded corners.
    RoundedRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        radius: BorderRadius,
    },
    /// Box shadow (blurred rect behind element).
    Shadow {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        blur: f32,
        color: Color,
        opacity: f32,
    },
    /// Linear gradient fill.
    Gradient {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        angle: f32,
        stops: Vec<(f32, Color)>,
        opacity: f32,
    },
    /// Image (already loaded — pixel data is passed inline).
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image: crate::image::Image,
    },
    /// Text (single line). x/y is the baseline-adjacent top-left.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: Color,
        font_size: f32,
        font_weight: u16,
        italic: bool,
        family: String,
    },
}

/// RGBA pixel buffer.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0xFF; (width * height * 4) as usize],
        }
    }

    /// Create a transparent canvas (all pixels 0x00).
    pub fn new_transparent(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0x00; (width * height * 4) as usize],
        }
    }

    pub fn clear_white(&mut self) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
            px[3] = 255;
        }
    }

    /// Blend a filled rect onto the canvas (alpha-composited).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 || w.is_nan() || h.is_nan() || x.is_nan() || y.is_nan() {
            return;
        }
        let x0 = x.max(0.0).floor() as i32;
        let y0 = y.max(0.0).floor() as i32;
        let x1 = ((x + w).min(self.width as f32)).ceil() as i32;
        let y1 = ((y + h).min(self.height as f32)).ceil() as i32;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for py in y0..y1 {
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for px in x0..x1 {
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                blend(&mut self.pixels[idx..idx + 4], color);
            }
        }
    }

    /// Fill a rounded rect. Simple approach: fill the bounding rect, then
    /// carve out corners by drawing transparent circles. For small radii
    /// this looks good enough.
    pub fn fill_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        radius: BorderRadius,
    ) {
        // Guard against NaN/negative dimensions.
        if w <= 0.0 || h <= 0.0 || w.is_nan() || h.is_nan() || x.is_nan() || y.is_nan() {
            return;
        }
        // If no radius, just fill a normal rect.
        if radius.top_left == 0.0
            && radius.top_right == 0.0
            && radius.bottom_left == 0.0
            && radius.bottom_right == 0.0
        {
            self.fill_rect(x, y, w, h, color);
            return;
        }
        // Fill the rect, but skip pixels outside the rounded corners.
        let x0 = x.max(0.0).floor() as i32;
        let y0 = y.max(0.0).floor() as i32;
        let x1 = ((x + w).min(self.width as f32)).ceil() as i32;
        let y1 = ((y + h).min(self.height as f32)).ceil() as i32;
        for py in y0..y1 {
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for px in x0..x1 {
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                // Check corner regions.
                let rel_x = px as f32 - x;
                let rel_y = py as f32 - y;
                let mut skip = false;
                // Top-left corner.
                if rel_x < radius.top_left && rel_y < radius.top_left {
                    let dx = radius.top_left - rel_x;
                    let dy = radius.top_left - rel_y;
                    if dx * dx + dy * dy > radius.top_left * radius.top_left {
                        skip = true;
                    }
                }
                // Top-right corner.
                if rel_x > w - radius.top_right && rel_y < radius.top_right {
                    let dx = rel_x - (w - radius.top_right);
                    let dy = radius.top_right - rel_y;
                    if dx * dx + dy * dy > radius.top_right * radius.top_right {
                        skip = true;
                    }
                }
                // Bottom-left corner.
                if rel_x < radius.bottom_left && rel_y > h - radius.bottom_left {
                    let dx = radius.bottom_left - rel_x;
                    let dy = rel_y - (h - radius.bottom_left);
                    if dx * dx + dy * dy > radius.bottom_left * radius.bottom_left {
                        skip = true;
                    }
                }
                // Bottom-right corner.
                if rel_x > w - radius.bottom_right && rel_y > h - radius.bottom_right {
                    let dx = rel_x - (w - radius.bottom_right);
                    let dy = rel_y - (h - radius.bottom_right);
                    if dx * dx + dy * dy > radius.bottom_right * radius.bottom_right {
                        skip = true;
                    }
                }
                if skip {
                    continue;
                }
                let idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                blend(&mut self.pixels[idx..idx + 4], color);
            }
        }
    }

    /// Draw a single glyph (alpha mask) at (x, y), filled with `color`.
    pub fn draw_glyph(&mut self, x: i32, y: i32, glyph: &Glyph, color: Color) {
        let w = glyph.width;
        let h = glyph.height;
        for gy in 0..h {
            for gx in 0..w {
                let alpha = glyph.mask[(gy * w + gx) as usize];
                if alpha == 0 {
                    continue;
                }
                let px = x + gx + glyph.offset_x;
                let py = y + gy + glyph.offset_y;
                if px < 0 || px >= self.width as i32 || py < 0 || py >= self.height as i32 {
                    continue;
                }
                let idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                let a = (alpha as u32 * color.a as u32) / 255;
                let c = Color {
                    a: a as u8,
                    ..color
                };
                blend(&mut self.pixels[idx..idx + 4], c);
            }
        }
    }

    /// Draw a blurred shadow rect. Simple approach: draw the rect multiple
    /// times with decreasing alpha at increasing offsets.
    pub fn draw_shadow(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        blur: f32,
        color: Color,
        opacity: f32,
    ) {
        if blur <= 0.0 {
            let mut c = color;
            c.a = ((c.a as f32 * opacity) as u8).max(1);
            self.fill_rect(x, y, w, h, c);
            return;
        }
        // Multi-pass blur: draw the rect at several offsets with decreasing alpha.
        let passes = (blur / 2.0).max(2.0).min(10.0) as i32;
        for i in 0..passes {
            let t = i as f32 / passes as f32;
            let offset = blur * t;
            let alpha = (color.a as f32 * opacity * (1.0 - t) * 0.5) as u8;
            if alpha == 0 {
                continue;
            }
            let c = Color { a: alpha, ..color };
            self.fill_rect(
                x - offset,
                y - offset,
                w + offset * 2.0,
                h + offset * 2.0,
                c,
            );
        }
    }

    /// Draw a linear gradient.
    pub fn draw_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        angle: f32,
        stops: &[(f32, Color)],
        opacity: f32,
    ) {
        if stops.is_empty() {
            return;
        }
        let rad = (angle - 90.0).to_radians();
        let dx = rad.cos();
        let dy = rad.sin();
        let x0i = x.max(0.0).floor() as i32;
        let y0i = y.max(0.0).floor() as i32;
        let x1i = ((x + w).min(self.width as f32)).ceil() as i32;
        let y1i = ((y + h).min(self.height as f32)).ceil() as i32;
        let len = (w * dx.abs() + h * dy.abs()).max(1.0);
        for py in y0i..y1i {
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for px in x0i..x1i {
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let rel_x = px as f32 - x;
                let rel_y = py as f32 - y;
                let proj = (rel_x * dx + rel_y * dy) / len;
                let t = proj.clamp(0.0, 1.0);
                let mut color = stops[0].1;
                for i in 0..stops.len() - 1 {
                    let (p0, c0) = stops[i];
                    let (p1, c1) = stops[i + 1];
                    if t >= p0 && t <= p1 {
                        let local_t = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
                        color = lerp_color(c0, c1, local_t);
                        break;
                    }
                    if t > p1 {
                        color = c1;
                    }
                }
                let mut c = color;
                if opacity < 1.0 {
                    c.a = ((c.a as f32 * opacity) as u8).max(1);
                }
                let idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                blend(&mut self.pixels[idx..idx + 4], c);
            }
        }
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: (a.r as f32 + (b.r as f32 - a.r as f32) * t) as u8,
        g: (a.g as f32 + (b.g as f32 - a.g as f32) * t) as u8,
        b: (a.b as f32 + (b.b as f32 - a.b as f32) * t) as u8,
        a: (a.a as f32 + (b.a as f32 - a.a as f32) * t) as u8,
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
    let out_r = (src.r as f32 * sa + dst[0] as f32 * da * (1.0 - sa)) / out_a;
    let out_g = (src.g as f32 * sa + dst[1] as f32 * da * (1.0 - sa)) / out_a;
    let out_b = (src.b as f32 * sa + dst[2] as f32 * da * (1.0 - sa)) / out_a;
    dst[0] = out_r as u8;
    dst[1] = out_g as u8;
    dst[2] = out_b as u8;
    dst[3] = (out_a * 255.0) as u8;
}

/// A rasterized glyph — alpha mask + offset.
pub struct Glyph {
    pub width: i32,
    pub height: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub mask: Vec<u8>,
}

/// Font rasterizer — wraps ab_glyph.
pub struct FontRasterizer {
    sans: ab_glyph::FontVec,
    mono: ab_glyph::FontVec,
}

impl FontRasterizer {
    pub fn new() -> anyhow::Result<Self> {
        let sans_paths: &[&str] = &[
            // Linux
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/chinese/LiberationSans-Regular.ttf",
            // macOS
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/Library/Fonts/Arial.ttf",
            // Windows
            "C:\\Windows\\Fonts\\arial.ttf",
            "C:\\Windows\\Fonts\\arialbd.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\tahoma.ttf",
        ];
        let mono_paths: &[&str] = &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/System/Library/Fonts/Menlo.ttc",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\lucon.ttf",
        ];
        let sans = load_font(sans_paths)?;
        let mono = load_font(mono_paths)
            .unwrap_or_else(|_| load_font(sans_paths).expect("sans font must be loadable twice"));
        Ok(Self { sans, mono })
    }

    pub fn rasterize(
        &self,
        c: char,
        size: f32,
        family: &str,
        bold: bool,
        italic: bool,
    ) -> Option<Glyph> {
        use ab_glyph::{Font, FontVec, ScaleFont};
        let font: &FontVec =
            if family.contains("mono") || family.contains("courier") || family.contains("consol") {
                &self.mono
            } else {
                &self.sans
            };
        let scale = ab_glyph::PxScale::from(size);
        let scaled = font.as_scaled(scale);
        let glyph_id = font.glyph_id(c);
        if glyph_id.0 == 0 {
            return None;
        }
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
        let advance = scaled.h_advance(glyph_id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let mut width = bounds.width() as i32;
            let height = bounds.height() as i32;
            if width <= 0 || height <= 0 {
                return Some(Glyph {
                    width: 1,
                    height: 1,
                    offset_x: 0,
                    offset_y: 0,
                    mask: vec![0],
                });
            }
            let mut mask = vec![0u8; (width * height) as usize];
            outline.draw(|x, y, cov| {
                let idx = (y as i32 * width + x as i32) as usize;
                if idx < mask.len() {
                    mask[idx] = (cov * 255.0) as u8;
                }
            });
            if italic {
                let skew = (size * 0.2) as i32;
                let new_width = width + skew;
                let mut new_mask = vec![0u8; (new_width * height) as usize];
                for y in 0..height {
                    let shift = (skew * y) / height;
                    for x in 0..width {
                        let new_x = x + shift;
                        if new_x < new_width {
                            let src_idx = (y * width + x) as usize;
                            let dst_idx = (y * new_width + new_x) as usize;
                            if dst_idx < new_mask.len() {
                                new_mask[dst_idx] = mask[src_idx];
                            }
                        }
                    }
                }
                width = new_width;
                mask = new_mask;
            }
            if bold {
                let new_width = width + 1;
                let mut new_mask = vec![0u8; (new_width * height) as usize];
                for y in 0..height {
                    for x in 0..width {
                        let src = mask[(y * width + x) as usize];
                        let dst_idx = (y * new_width + x) as usize;
                        let dst_idx2 = (y * new_width + x + 1) as usize;
                        if dst_idx < new_mask.len() {
                            new_mask[dst_idx] = new_mask[dst_idx].max(src);
                        }
                        if dst_idx2 < new_mask.len() {
                            new_mask[dst_idx2] = new_mask[dst_idx2].max(src);
                        }
                    }
                }
                width = new_width;
                mask = new_mask;
            }
            Some(Glyph {
                width,
                height,
                offset_x: bounds.min.x as i32,
                offset_y: bounds.min.y as i32,
                mask,
            })
        } else {
            // Whitespace / control glyphs carry no outline but still have a
            // horizontal advance. Return a transparent glyph whose width
            // equals that advance so the paint loop advances the caret and
            // inter-word spaces are not collapsed into zero width.
            let w = advance.round().max(1.0) as i32;
            let h = 1i32;
            Some(Glyph {
                width: w,
                height: h,
                offset_x: 0,
                offset_y: 0,
                mask: vec![0u8; (w * h) as usize],
            })
        }
    }
}

fn load_font(paths: &[&str]) -> anyhow::Result<ab_glyph::FontVec> {
    for p in paths {
        if let Ok(data) = std::fs::read(p) {
            if let Ok(font) = ab_glyph::FontVec::try_from_vec(data) {
                return Ok(font);
            }
        }
    }
    // Last resort: try to find ANY .ttf file on the system.
    #[cfg(unix)]
    {
        if let Ok(entries) = std::fs::read_dir("/usr/share/fonts") {
            for entry in entries.flatten() {
                if let Ok(font) = try_load_dir(&entry.path()) {
                    return Ok(font);
                }
            }
        }
    }
    anyhow::bail!("no font found in paths: {:?}", paths)
}

#[cfg(unix)]
fn try_load_dir(path: &std::path::Path) -> anyhow::Result<ab_glyph::FontVec> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path)?.flatten() {
            let p = entry.path();
            if p.extension()
                .map(|e| e == "ttf" || e == "otf" || e == "TTF" || e == "OTF")
                .unwrap_or(false)
            {
                if let Ok(data) = std::fs::read(&p) {
                    if let Ok(font) = ab_glyph::FontVec::try_from_vec(data) {
                        return Ok(font);
                    }
                }
            }
            if p.is_dir() {
                if let Ok(font) = try_load_dir(&p) {
                    return Ok(font);
                }
            }
        }
    }
    anyhow::bail!("no font in directory")
}

/// Render all paint commands onto the canvas.
pub fn paint(commands: &[PaintCommand], canvas: &mut Canvas, rasterizer: &FontRasterizer) {
    for cmd in commands {
        match cmd {
            PaintCommand::Rect { x, y, w, h, color } => {
                canvas.fill_rect(*x, *y, *w, *h, *color);
            }
            PaintCommand::RoundedRect {
                x,
                y,
                w,
                h,
                color,
                radius,
            } => {
                canvas.fill_rounded_rect(*x, *y, *w, *h, *color, *radius);
            }
            PaintCommand::Shadow {
                x,
                y,
                w,
                h,
                blur,
                color,
                opacity,
            } => {
                canvas.draw_shadow(*x, *y, *w, *h, *blur, *color, *opacity);
            }
            PaintCommand::Gradient {
                x,
                y,
                w,
                h,
                angle,
                stops,
                opacity,
            } => {
                canvas.draw_gradient(*x, *y, *w, *h, *angle, stops, *opacity);
            }
            PaintCommand::Image { x, y, w, h, image } => {
                crate::image::draw_image_to_canvas(canvas, image, *x, *y, *w, *h);
            }
            PaintCommand::Text {
                x,
                y,
                text,
                color,
                font_size,
                font_weight,
                italic,
                family,
            } => {
                let mut cx = *x as i32;
                let baseline = *y as i32 + (*font_size * 0.8) as i32;
                let bold = *font_weight >= 600;
                for c in text.chars() {
                    if let Some(glyph) = rasterizer.rasterize(c, *font_size, family, bold, *italic)
                    {
                        canvas.draw_glyph(cx, baseline, &glyph, *color);
                        cx += glyph.width + glyph.offset_x;
                    }
                }
            }
        }
    }
}
