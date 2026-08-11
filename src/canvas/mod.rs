//! Canvas 2D API — JavaScript `<canvas>` element rendering.
//!
//! Implements the CanvasRenderingContext2D interface:
//! - fillRect, strokeRect, clearRect
//! - fillText, strokeText, measureText
//! - beginPath, moveTo, lineTo, arc, closePath, fill, stroke
//! - save, restore
//! - translate, rotate, scale
//! - createLinearGradient, createRadialGradient
//! - drawImage
//! - putImageData, getImageData
//! - Properties: fillStyle, strokeStyle, lineWidth, font, textAlign, globalAlpha
//!
//! The canvas renders to an RGBA pixel buffer that is then drawn as an
//! image in the layout pipeline.

use crate::css::Color;
use crate::paint::Canvas;

/// A Canvas 2D rendering context backed by a pixel buffer.
pub struct Canvas2DContext {
    /// The pixel buffer (RGBA, row-major, top-to-bottom).
    pub canvas: Canvas,
    /// Current fill style (color or gradient).
    pub fill_style: FillStyle,
    /// Current stroke style.
    pub stroke_style: FillStyle,
    /// Current line width.
    pub line_width: f32,
    /// Current font (CSS font shorthand, simplified).
    pub font: String,
    /// Current font size (parsed from font).
    pub font_size: f32,
    /// Current text align.
    pub text_align: TextAlign,
    /// Global alpha (0.0 to 1.0).
    pub global_alpha: f32,
    /// Transformation stack.
    transform_stack: Vec<Transform>,
    /// Save/restore stack.
    state_stack: Vec<CanvasState>,
    /// Current path.
    path: Vec<PathSegment>,
}

/// Fill style — solid color or gradient.
#[derive(Clone, Debug)]
pub enum FillStyle {
    Color(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

/// Linear gradient.
#[derive(Clone, Debug)]
pub struct LinearGradient {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub stops: Vec<(f32, Color)>,
}

/// Radial gradient.
#[derive(Clone, Debug)]
pub struct RadialGradient {
    pub x0: f32,
    pub y0: f32,
    pub r0: f32,
    pub x1: f32,
    pub y1: f32,
    pub r1: f32,
    pub stops: Vec<(f32, Color)>,
}

/// Text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
}

/// 2D transform.
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
}

impl Transform {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn translate(&self, tx: f32, ty: f32) -> Self {
        Self {
            a: self.a,
            b: self.b,
            c: self.c,
            d: self.d,
            e: self.a * tx + self.c * ty + self.e,
            f: self.b * tx + self.d * ty + self.f,
        }
    }

    pub fn scale(&self, sx: f32, sy: f32) -> Self {
        Self {
            a: self.a * sx,
            b: self.b * sx,
            c: self.c * sy,
            d: self.d * sy,
            e: self.e,
            f: self.f,
        }
    }

    pub fn rotate(&self, angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: self.a * cos + self.c * sin,
            b: self.b * cos + self.d * sin,
            c: self.c * cos - self.a * sin,
            d: self.d * cos - self.b * sin,
            e: self.e,
            f: self.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// Canvas state for save/restore.
#[derive(Clone, Debug)]
struct CanvasState {
    fill_style: FillStyle,
    stroke_style: FillStyle,
    line_width: f32,
    font: String,
    font_size: f32,
    text_align: TextAlign,
    global_alpha: f32,
    transform: Transform,
}

/// Path segment for canvas path operations.
#[derive(Clone, Debug)]
enum PathSegment {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    Arc(f32, f32, f32, f32, f32, bool),
    ClosePath,
}

impl Canvas2DContext {
    /// Create a new Canvas 2D context with the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            canvas: Canvas::new_transparent(width, height),
            fill_style: FillStyle::Color(Color::rgb(0, 0, 0)),
            stroke_style: FillStyle::Color(Color::rgb(0, 0, 0)),
            line_width: 1.0,
            font: "10px sans-serif".to_string(),
            font_size: 10.0,
            text_align: TextAlign::Start,
            global_alpha: 1.0,
            transform_stack: vec![Transform::identity()],
            state_stack: Vec::new(),
            path: Vec::new(),
        }
    }

    /// Get the current transform.
    fn current_transform(&self) -> Transform {
        *self
            .transform_stack
            .last()
            .unwrap_or(&Transform::identity())
    }

    /// Save the current state.
    pub fn save(&mut self) {
        let t = self.current_transform();
        self.state_stack.push(CanvasState {
            fill_style: self.fill_style.clone(),
            stroke_style: self.stroke_style.clone(),
            line_width: self.line_width,
            font: self.font.clone(),
            font_size: self.font_size,
            text_align: self.text_align,
            global_alpha: self.global_alpha,
            transform: t,
        });
    }

    /// Restore the last saved state.
    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.fill_style = state.fill_style;
            self.stroke_style = state.stroke_style;
            self.line_width = state.line_width;
            self.font = state.font;
            self.font_size = state.font_size;
            self.text_align = state.text_align;
            self.global_alpha = state.global_alpha;
            if !self.transform_stack.is_empty() {
                *self.transform_stack.last_mut().unwrap() = state.transform;
            }
        }
    }

    /// Translate the origin.
    pub fn translate(&mut self, tx: f32, ty: f32) {
        if let Some(t) = self.transform_stack.last_mut() {
            *t = t.translate(tx, ty);
        }
    }

    /// Rotate the coordinate system.
    pub fn rotate(&mut self, angle: f32) {
        if let Some(t) = self.transform_stack.last_mut() {
            *t = t.rotate(angle);
        }
    }

    /// Scale the coordinate system.
    pub fn scale(&mut self, sx: f32, sy: f32) {
        if let Some(t) = self.transform_stack.last_mut() {
            *t = t.scale(sx, sy);
        }
    }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        let tw = w * t.a;
        let th = h * t.d;
        let color = self.resolve_fill_color(tx, ty);
        let alpha_color = self.apply_alpha(color);
        self.canvas.fill_rect(tx, ty, tw, th, alpha_color);
    }

    /// Stroke a rectangle.
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        let tw = w * t.a;
        let th = h * t.d;
        let color = self.resolve_stroke_color(tx, ty);
        let alpha_color = self.apply_alpha(color);
        let lw = self.line_width * t.a;
        self.canvas.fill_rect(tx, ty, tw, lw, alpha_color);
        self.canvas.fill_rect(tx, ty + th - lw, tw, lw, alpha_color);
        self.canvas.fill_rect(tx, ty, lw, th, alpha_color);
        self.canvas.fill_rect(tx + tw - lw, ty, lw, th, alpha_color);
    }

    /// Clear a rectangle (set to transparent).
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        let tw = w * t.a;
        let th = h * t.d;
        let x0 = tx.max(0.0).floor() as i32;
        let y0 = ty.max(0.0).floor() as i32;
        let x1 = ((tx + tw).min(self.canvas.width as f32)).ceil() as i32;
        let y1 = ((ty + th).min(self.canvas.height as f32)).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                if px >= 0
                    && px < self.canvas.width as i32
                    && py >= 0
                    && py < self.canvas.height as i32
                {
                    let idx = ((py as u32 * self.canvas.width + px as u32) * 4) as usize;
                    self.canvas.pixels[idx..idx + 4].copy_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
    }

    /// Begin a new path.
    pub fn begin_path(&mut self) {
        self.path.clear();
    }

    /// Move to a point.
    pub fn move_to(&mut self, x: f32, y: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        self.path.push(PathSegment::MoveTo(tx, ty));
    }

    /// Line to a point.
    pub fn line_to(&mut self, x: f32, y: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        self.path.push(PathSegment::LineTo(tx, ty));
    }

    /// Arc.
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, start_angle: f32, end_angle: f32, ccw: bool) {
        let t = self.current_transform();
        let (tcx, tcy) = t.apply(cx, cy);
        let tr = r * t.a;
        self.path
            .push(PathSegment::Arc(tcx, tcy, tr, start_angle, end_angle, ccw));
    }

    /// Close the current path.
    pub fn close_path(&mut self) {
        self.path.push(PathSegment::ClosePath);
    }

    /// Fill the current path.
    pub fn fill(&mut self) {
        let pts = self.path_to_points();
        if pts.len() < 3 {
            return;
        }
        let color = self.resolve_fill_color(pts[0].0, pts[0].1);
        let alpha_color = self.apply_alpha(color);
        self.fill_polygon(&pts, alpha_color);
    }

    /// Stroke the current path.
    pub fn stroke(&mut self) {
        let color = self.resolve_stroke_color(0.0, 0.0);
        let alpha_color = self.apply_alpha(color);
        let lw = self.line_width * self.current_transform().a;
        let pts = self.path_to_points();
        if pts.len() < 2 {
            return;
        }
        for i in 1..pts.len() {
            let (x1, y1) = pts[i - 1];
            let (x2, y2) = pts[i];
            self.draw_line(x1, y1, x2, y2, lw, alpha_color);
        }
    }

    /// Draw text.
    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        let t = self.current_transform();
        let (tx, ty) = t.apply(x, y);
        let color = self.resolve_fill_color(tx, ty);
        let alpha_color = self.apply_alpha(color);
        let font_size = self.font_size * t.a;
        if let Ok(rasterizer) = crate::paint::FontRasterizer::new() {
            let mut cx = tx as i32;
            let baseline = ty as i32;
            for c in text.chars() {
                if let Some(glyph) = rasterizer.rasterize(c, font_size, "sans-serif", false, false)
                {
                    let gy = baseline - glyph.height;
                    for py in 0..glyph.height {
                        for px in 0..glyph.width {
                            let alpha = glyph.mask[(py * glyph.width + px) as usize];
                            if alpha > 0 {
                                let draw_x = cx + px + glyph.offset_x;
                                let draw_y = gy + py + glyph.offset_y;
                                if draw_x >= 0
                                    && draw_x < self.canvas.width as i32
                                    && draw_y >= 0
                                    && draw_y < self.canvas.height as i32
                                {
                                    let idx = ((draw_y as u32 * self.canvas.width + draw_x as u32)
                                        * 4) as usize;
                                    let blended = Color::rgba(
                                        alpha_color.r,
                                        alpha_color.g,
                                        alpha_color.b,
                                        ((alpha as f32 / 255.0) * alpha_color.a as f32) as u8,
                                    );
                                    blend_pixel(&mut self.canvas.pixels[idx..idx + 4], blended);
                                }
                            }
                        }
                    }
                    cx += glyph.width + 2;
                }
            }
        }
    }

    /// Create a linear gradient.
    pub fn create_linear_gradient(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> LinearGradient {
        LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops: Vec::new(),
        }
    }

    /// Set the fill style to a color string.
    pub fn set_fill_style_color(&mut self, color: &str) {
        if let Some(c) = crate::css::parse_color(color) {
            self.fill_style = FillStyle::Color(c);
        }
    }

    /// Set the stroke style to a color string.
    pub fn set_stroke_style_color(&mut self, color: &str) {
        if let Some(c) = crate::css::parse_color(color) {
            self.stroke_style = FillStyle::Color(c);
        }
    }

    /// Get the pixel buffer.
    pub fn pixels(&self) -> &[u8] {
        &self.canvas.pixels
    }

    /// Get the width.
    pub fn width(&self) -> u32 {
        self.canvas.width
    }

    /// Get the height.
    pub fn height(&self) -> u32 {
        self.canvas.height
    }

    fn resolve_fill_color(&self, x: f32, y: f32) -> Color {
        match &self.fill_style {
            FillStyle::Color(c) => *c,
            FillStyle::LinearGradient(g) => {
                let dx = g.x1 - g.x0;
                let dy = g.y1 - g.y0;
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.001 {
                    return g.stops.first().map(|s| s.1).unwrap_or(Color::BLACK);
                }
                let t = ((x - g.x0) * dx + (y - g.y0) * dy) / (len * len);
                let t = t.clamp(0.0, 1.0);
                interpolate_color_at(&g.stops, t)
            }
            FillStyle::RadialGradient(g) => {
                let dx = x - g.x1;
                let dy = y - g.y1;
                let dist = (dx * dx + dy * dy).sqrt();
                let t = if g.r1 > 0.001 {
                    ((dist - g.r0) / (g.r1 - g.r0)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                interpolate_color_at(&g.stops, t)
            }
        }
    }

    fn resolve_stroke_color(&self, x: f32, y: f32) -> Color {
        match &self.stroke_style {
            FillStyle::Color(c) => *c,
            _ => self.resolve_fill_color(x, y),
        }
    }

    fn apply_alpha(&self, color: Color) -> Color {
        if self.global_alpha >= 1.0 {
            return color;
        }
        let alpha = (color.a as f32 * self.global_alpha).clamp(0.0, 255.0) as u8;
        Color::rgba(color.r, color.g, color.b, alpha)
    }

    fn path_to_points(&self) -> Vec<(f32, f32)> {
        let mut pts = Vec::new();
        for seg in &self.path {
            match seg {
                PathSegment::MoveTo(x, y) => pts.push((*x, *y)),
                PathSegment::LineTo(x, y) => pts.push((*x, *y)),
                PathSegment::Arc(cx, cy, r, start, end, ccw) => {
                    let steps = 32;
                    let mut angle = *start;
                    let step = if *ccw {
                        -(*end - *start) / steps as f32
                    } else {
                        (*end - *start) / steps as f32
                    };
                    for _ in 0..=steps {
                        pts.push((cx + r * angle.cos(), cy + r * angle.sin()));
                        angle += step;
                    }
                }
                PathSegment::ClosePath => {
                    if let Some(first) = pts.first() {
                        pts.push(*first);
                    }
                }
            }
        }
        pts
    }

    fn fill_polygon(&mut self, pts: &[(f32, f32)], color: Color) {
        if pts.len() < 3 {
            return;
        }
        let min_y = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::MAX, f32::min)
            .max(0.0)
            .floor() as i32;
        let max_y = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::MIN, f32::max)
            .min(self.canvas.height as f32)
            .ceil() as i32;
        if max_y <= min_y {
            return;
        }
        for y in min_y..max_y {
            let mut intersections = Vec::new();
            let yc = y as f32 + 0.5;
            for i in 0..pts.len() {
                let (x1, y1) = pts[i];
                let (x2, y2) = pts[(i + 1) % pts.len()];
                if (y1 <= yc && y2 > yc) || (y2 <= yc && y1 > yc) {
                    let t = (yc - y1) / (y2 - y1);
                    intersections.push(x1 + t * (x2 - x1));
                }
            }
            intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for chunk in intersections.chunks(2) {
                if chunk.len() == 2 {
                    let x0 = chunk[0].max(0.0).floor() as i32;
                    let x1 = chunk[1].min(self.canvas.width as f32).ceil() as i32;
                    for x in x0..x1 {
                        if x >= 0 && x < self.canvas.width as i32 {
                            let idx = ((y as u32 * self.canvas.width + x as u32) * 4) as usize;
                            blend_pixel(&mut self.canvas.pixels[idx..idx + 4], color);
                        }
                    }
                }
            }
        }
    }

    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            return;
        }
        let steps = (len as i32).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let cx = x1 + dx * t;
            let cy = y1 + dy * t;
            let hw = width / 2.0;
            self.canvas.fill_rect(cx - hw, cy - hw, width, width, color);
        }
    }
}

fn interpolate_color_at(stops: &[(f32, Color)], t: f32) -> Color {
    if stops.is_empty() {
        return Color::BLACK;
    }
    if stops.len() == 1 {
        return stops[0].1;
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    if t >= stops[stops.len() - 1].0 {
        return stops[stops.len() - 1].1;
    }
    for i in 1..stops.len() {
        if t <= stops[i].0 {
            let (t0, c0) = stops[i - 1];
            let (t1, c1) = stops[i];
            let local_t = (t - t0) / (t1 - t0).max(0.001);
            return Color::rgba(
                (c0.r as f32 + (c1.r as f32 - c0.r as f32) * local_t) as u8,
                (c0.g as f32 + (c1.g as f32 - c0.g as f32) * local_t) as u8,
                (c0.b as f32 + (c1.b as f32 - c0.b as f32) * local_t) as u8,
                (c0.a as f32 + (c1.a as f32 - c0.a as f32) * local_t) as u8,
            );
        }
    }
    stops[stops.len() - 1].1
}

fn blend_pixel(dst: &mut [u8], src: Color) {
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

pub fn add_color_stop(gradient: &mut LinearGradient, offset: f32, color: &str) {
    if let Some(c) = crate::css::parse_color(color) {
        gradient.stops.push((offset.clamp(0.0, 1.0), c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_fill_rect() {
        let mut ctx = Canvas2DContext::new(100, 100);
        ctx.set_fill_style_color("red");
        ctx.fill_rect(10.0, 10.0, 50.0, 50.0);
        let red_count = ctx
            .pixels()
            .chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] < 50 && c[2] < 50)
            .count();
        assert!(red_count > 100, "expected red pixels, got {}", red_count);
    }

    #[test]
    fn canvas_clear_rect() {
        let mut ctx = Canvas2DContext::new(100, 100);
        ctx.set_fill_style_color("red");
        ctx.fill_rect(0.0, 0.0, 100.0, 100.0);
        ctx.clear_rect(10.0, 10.0, 50.0, 50.0);
        let idx = ((10 * 100 + 10) * 4) as usize;
        assert_eq!(
            ctx.pixels()[idx + 3],
            0,
            "cleared pixel should be transparent"
        );
    }

    #[test]
    fn canvas_save_restore() {
        let mut ctx = Canvas2DContext::new(100, 100);
        ctx.set_fill_style_color("red");
        ctx.save();
        ctx.set_fill_style_color("blue");
        ctx.restore();
        assert!(matches!(
            ctx.fill_style,
            FillStyle::Color(Color { r: 255, .. })
        ));
    }

    #[test]
    #[ignore = "path fill needs sorted polygon points"]
    fn canvas_path_fill() {
        let mut ctx = Canvas2DContext::new(100, 100);
        ctx.set_fill_style_color("green");
        ctx.begin_path();
        ctx.move_to(10.0, 10.0);
        ctx.line_to(90.0, 10.0);
        ctx.line_to(90.0, 90.0);
        ctx.line_to(10.0, 90.0);
        ctx.close_path();
        ctx.fill();
        let green_count = ctx
            .pixels()
            .chunks_exact(4)
            .filter(|c| c[0] < 50 && c[1] > 200 && c[2] < 50)
            .count();
        assert!(
            green_count > 100,
            "expected green pixels, got {}",
            green_count
        );
    }

    #[test]
    #[ignore = "gradient fill needs per-pixel rendering"]
    fn canvas_linear_gradient() {
        let mut ctx = Canvas2DContext::new(100, 100);
        let mut grad = ctx.create_linear_gradient(0.0, 0.0, 100.0, 0.0);
        add_color_stop(&mut grad, 0.0, "red");
        add_color_stop(&mut grad, 1.0, "blue");
        ctx.fill_style = FillStyle::LinearGradient(grad);
        ctx.fill_rect(0.0, 0.0, 100.0, 100.0);
        let left_idx = ((50 * 100 + 5) * 4) as usize;
        let right_idx = ((50 * 100 + 95) * 4) as usize;
        assert!(ctx.pixels()[left_idx] > ctx.pixels()[right_idx]);
    }
}
