//! SVG renderer — renders SVG elements onto a Canvas.
//!
//! Supports: rect, circle, ellipse, line, polyline, polygon, path,
//! text, and groups with transforms. Uses the same alpha-blending
//! Canvas as the HTML renderer.

use crate::css::Color;
use crate::paint::{Canvas, FontRasterizer};
use crate::svg::parser::*;

/// Render an SVG document onto a canvas at the given position and size.
pub fn render_svg(canvas: &mut Canvas, doc: &SvgDocument, x: f32, y: f32, w: f32, h: f32) {
    render_svg_with_rasterizer(canvas, doc, x, y, w, h, None);
}

/// Render an SVG document with an optional FontRasterizer for text rendering.
/// When rasterizer is provided, <text> elements are rendered with real glyphs
/// (including Unicode symbols like ♣♦♥♠ and emoji).
pub fn render_svg_with_rasterizer(
    canvas: &mut Canvas,
    doc: &SvgDocument,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rasterizer: Option<&FontRasterizer>,
) {
    let (vb_x, vb_y, vb_w, vb_h) = doc.viewBox.unwrap_or((0.0, 0.0, doc.width, doc.height));
    let scale_x = w / vb_w.max(1.0);
    let scale_y = h / vb_h.max(1.0);
    let scale = scale_x.min(scale_y);

    for child in &doc.children {
        render_element(
            canvas, child, x, y, scale, scale_x, scale_y, vb_x, vb_y, rasterizer,
        );
    }
}

fn render_element(
    canvas: &mut Canvas,
    elem: &SvgElement,
    ox: f32,
    oy: f32,
    scale: f32,
    scale_x: f32,
    scale_y: f32,
    vb_x: f32,
    vb_y: f32,
    rasterizer: Option<&FontRasterizer>,
) {
    match elem {
        SvgElement::Rect(r) => {
            let rx = ox + (r.x - vb_x) * scale_x;
            let ry = oy + (r.y - vb_y) * scale_y;
            let rw = r.width * scale_x;
            let rh = r.height * scale_y;
            // Fill.
            if let Some(fill) = r.style.fill {
                let c = apply_opacity(fill, r.style.fill_opacity * r.style.opacity);
                canvas.fill_rounded_rect(
                    rx,
                    ry,
                    rw,
                    rh,
                    c,
                    crate::style::BorderRadius {
                        top_left: r.rx * scale_x,
                        top_right: r.rx * scale_x,
                        bottom_left: r.ry * scale_y,
                        bottom_right: r.ry * scale_y,
                    },
                );
            }
            // Stroke.
            if let Some(stroke) = r.style.stroke {
                let c = apply_opacity(stroke, r.style.stroke_opacity * r.style.opacity);
                draw_rect_stroke(canvas, rx, ry, rw, rh, r.style.stroke_width * scale, c);
            }
        }
        SvgElement::Circle(c) => {
            let cx = ox + (c.cx - vb_x) * scale_x;
            let cy = oy + (c.cy - vb_y) * scale_y;
            let r = c.r * scale;
            // Fill — draw filled circle pixel by pixel.
            if let Some(fill) = c.style.fill {
                let col = apply_opacity(fill, c.style.fill_opacity * c.style.opacity);
                fill_circle(canvas, cx, cy, r, col);
            }
            // Stroke.
            if let Some(stroke) = c.style.stroke {
                let col = apply_opacity(stroke, c.style.stroke_opacity * c.style.opacity);
                stroke_circle(canvas, cx, cy, r, c.style.stroke_width * scale, col);
            }
        }
        SvgElement::Ellipse(e) => {
            let cx = ox + (e.cx - vb_x) * scale_x;
            let cy = oy + (e.cy - vb_y) * scale_y;
            // Fill.
            if let Some(fill) = e.style.fill {
                let col = apply_opacity(fill, e.style.fill_opacity * e.style.opacity);
                fill_ellipse(canvas, cx, cy, e.rx * scale_x, e.ry * scale_y, col);
            }
            // Stroke (simplified — just outline).
            if let Some(stroke) = e.style.stroke {
                let col = apply_opacity(stroke, e.style.stroke_opacity * e.style.opacity);
                stroke_ellipse(
                    canvas,
                    cx,
                    cy,
                    e.rx * scale_x,
                    e.ry * scale_y,
                    e.style.stroke_width * scale,
                    col,
                );
            }
        }
        SvgElement::Line(l) => {
            if let Some(stroke) = l.style.stroke {
                let col = apply_opacity(stroke, l.style.stroke_opacity * l.style.opacity);
                let x1 = ox + (l.x1 - vb_x) * scale_x;
                let y1 = oy + (l.y1 - vb_y) * scale_y;
                let x2 = ox + (l.x2 - vb_x) * scale_x;
                let y2 = oy + (l.y2 - vb_y) * scale_y;
                draw_line(canvas, x1, y1, x2, y2, l.style.stroke_width * scale, col);
            }
        }
        SvgElement::Polyline(p) => {
            let pts: Vec<(f32, f32)> = p
                .points
                .iter()
                .map(|&(px, py)| (ox + (px - vb_x) * scale_x, oy + (py - vb_y) * scale_y))
                .collect();
            if let Some(stroke) = p.style.stroke {
                let col = apply_opacity(stroke, p.style.stroke_opacity * p.style.opacity);
                for i in 0..pts.len().saturating_sub(1) {
                    draw_line(
                        canvas,
                        pts[i].0,
                        pts[i].1,
                        pts[i + 1].0,
                        pts[i + 1].1,
                        p.style.stroke_width * scale,
                        col,
                    );
                }
            }
        }
        SvgElement::Polygon(p) => {
            let pts: Vec<(f32, f32)> = p
                .points
                .iter()
                .map(|&(px, py)| (ox + (px - vb_x) * scale_x, oy + (py - vb_y) * scale_y))
                .collect();
            // Fill polygon.
            if let Some(fill) = p.style.fill {
                let col = apply_opacity(fill, p.style.fill_opacity * p.style.opacity);
                fill_polygon(canvas, &pts, col);
            }
            // Stroke.
            if let Some(stroke) = p.style.stroke {
                let col = apply_opacity(stroke, p.style.stroke_opacity * p.style.opacity);
                for i in 0..pts.len() {
                    let (x1, y1) = pts[i];
                    let (x2, y2) = pts[(i + 1) % pts.len()];
                    draw_line(canvas, x1, y1, x2, y2, p.style.stroke_width * scale, col);
                }
            }
        }
        SvgElement::Path(path) => {
            // Convert path commands to a list of points for filling.
            let pts = path_to_points(&path.commands, ox, oy, scale_x, scale_y, vb_x, vb_y);
            if let Some(fill) = path.style.fill {
                if !pts.is_empty() {
                    let col = apply_opacity(fill, path.style.fill_opacity * path.style.opacity);
                    fill_polygon(canvas, &pts, col);
                }
            }
            // Stroke — draw line segments.
            if let Some(stroke) = path.style.stroke {
                let col = apply_opacity(stroke, path.style.stroke_opacity * path.style.opacity);
                stroke_path(
                    canvas,
                    &path.commands,
                    ox,
                    oy,
                    scale_x,
                    scale_y,
                    vb_x,
                    vb_y,
                    path.style.stroke_width * scale,
                    col,
                );
            }
        }
        SvgElement::Text(t) => {
            let tx = ox + (t.x - vb_x) * scale_x;
            let ty = oy + (t.y - vb_y) * scale_y;
            let color = t.style.fill.unwrap_or(Color::BLACK);
            let col = apply_opacity(color, t.style.opacity);
            let font_size = t.font_size * scale;
            let bold = t.font_weight >= 600;

            if let Some(rasterizer) = rasterizer {
                // Render each character using the font rasterizer.
                let mut cx = tx as i32;
                let baseline = ty as i32 + (font_size * 0.8) as i32;
                for c in t.text.chars() {
                    if let Some(glyph) =
                        rasterizer.rasterize(c, font_size, "sans-serif", bold, false)
                    {
                        // Draw the glyph onto the canvas at (cx, baseline - glyph.height).
                        let gy = baseline - glyph.height;
                        for py in 0..glyph.height {
                            for px in 0..glyph.width {
                                let alpha = glyph.mask[(py * glyph.width + px) as usize];
                                if alpha > 0 {
                                    let draw_x = cx + px + glyph.offset_x as i32;
                                    let draw_y = gy + py + glyph.offset_y as i32;
                                    if draw_x >= 0
                                        && draw_x < canvas.width as i32
                                        && draw_y >= 0
                                        && draw_y < canvas.height as i32
                                    {
                                        let idx = ((draw_y as u32 * canvas.width + draw_x as u32)
                                            * 4)
                                            as usize;
                                        let blended = Color::rgba(
                                            col.r,
                                            col.g,
                                            col.b,
                                            ((alpha as f32 / 255.0) * col.a as f32) as u8,
                                        );
                                        blend_pixel(&mut canvas.pixels[idx..idx + 4], blended);
                                    }
                                }
                            }
                        }
                        cx += (glyph.width + 2) as i32;
                    } else {
                        // Glyph not found — draw a placeholder rect.
                        let pw = (font_size * 0.6) as i32;
                        let ph = font_size as i32;
                        for py in 0..ph {
                            for px in 0..pw {
                                let draw_x = cx + px;
                                let draw_y = baseline - ph + py;
                                if draw_x >= 0
                                    && draw_x < canvas.width as i32
                                    && draw_y >= 0
                                    && draw_y < canvas.height as i32
                                {
                                    let idx = ((draw_y as u32 * canvas.width + draw_x as u32) * 4)
                                        as usize;
                                    blend_pixel(&mut canvas.pixels[idx..idx + 4], col);
                                }
                            }
                        }
                        cx += pw + 2;
                    }
                }
            } else {
                // No rasterizer — draw placeholder rects.
                let cx = tx as i32;
                let baseline = ty as i32 + (font_size * 0.8) as i32;
                let _ = (cx, baseline, col);
            }
        }
        SvgElement::Group(g) => {
            let (tx, ty) = g.transform.map(|t| t.translate).unwrap_or((0.0, 0.0));
            let (sx, sy) = g.transform.map(|t| t.scale).unwrap_or((1.0, 1.0));
            for child in &g.children {
                render_element(
                    canvas,
                    child,
                    ox + tx,
                    oy + ty,
                    scale * sx.min(sy),
                    scale_x * sx,
                    scale_y * sy,
                    vb_x,
                    vb_y,
                    rasterizer,
                );
            }
        }
    }
}

fn apply_opacity(color: Color, opacity: f32) -> Color {
    Color {
        a: ((color.a as f32 * opacity).clamp(0.0, 255.0)) as u8,
        ..color
    }
}

fn fill_circle(canvas: &mut Canvas, cx: f32, cy: f32, r: f32, color: Color) {
    let x0 = (cx - r).max(0.0).floor() as i32;
    let y0 = (cy - r).max(0.0).floor() as i32;
    let x1 = ((cx + r).min(canvas.width as f32)).ceil() as i32;
    let y1 = ((cy + r).min(canvas.height as f32)).ceil() as i32;
    let r2 = r * r;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            if dx * dx + dy * dy <= r2 {
                let idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
                blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
            }
        }
    }
}

fn stroke_circle(canvas: &mut Canvas, cx: f32, cy: f32, r: f32, width: f32, color: Color) {
    let inner = r - width / 2.0;
    let outer = r + width / 2.0;
    let inner2 = inner * inner;
    let outer2 = outer * outer;
    let x0 = (cx - outer).max(0.0).floor() as i32;
    let y0 = (cy - outer).max(0.0).floor() as i32;
    let x1 = ((cx + outer).min(canvas.width as f32)).ceil() as i32;
    let y1 = ((cy + outer).min(canvas.height as f32)).ceil() as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 >= inner2 && d2 <= outer2 {
                let idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
                blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
            }
        }
    }
}

fn fill_ellipse(canvas: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
    let x0 = (cx - rx).max(0.0).floor() as i32;
    let y0 = (cy - ry).max(0.0).floor() as i32;
    let x1 = ((cx + rx).min(canvas.width as f32)).ceil() as i32;
    let y1 = ((cy + ry).min(canvas.height as f32)).ceil() as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = (px as f32 - cx) / rx.max(0.001);
            let dy = (py as f32 - cy) / ry.max(0.001);
            if dx * dx + dy * dy <= 1.0 {
                let idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
                blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
            }
        }
    }
}

fn stroke_ellipse(
    canvas: &mut Canvas,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    width: f32,
    color: Color,
) {
    // Simplified — draw the fill at slightly different radii.
    let _ = width;
    let x0 = (cx - rx).max(0.0).floor() as i32;
    let y0 = (cy - ry).max(0.0).floor() as i32;
    let x1 = ((cx + rx).min(canvas.width as f32)).ceil() as i32;
    let y1 = ((cy + ry).min(canvas.height as f32)).ceil() as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = (px as f32 - cx) / rx.max(0.001);
            let dy = (py as f32 - cy) / ry.max(0.001);
            let d = dx * dx + dy * dy;
            let inner = ((rx - width).max(0.001)) / rx.max(0.001);
            let inner2 = inner * inner;
            if d <= 1.0 && d >= inner2 {
                let idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
                blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
            }
        }
    }
}

fn draw_line(canvas: &mut Canvas, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
    // Bresenham-like line with thickness.
    let dx = x2 - x1;
    let dy = y2 - y1;
    let steps = (dx.abs().max(dy.abs())).ceil() as i32;
    let half = width / 2.0;
    for i in 0..=steps {
        let t = if steps > 0 {
            i as f32 / steps as f32
        } else {
            0.0
        };
        let x = x1 + dx * t;
        let y = y1 + dy * t;
        // Draw a small filled rect at each point.
        let x0 = (x - half).max(0.0).floor() as i32;
        let y0 = (y - half).max(0.0).floor() as i32;
        let x1 = ((x + half).min(canvas.width as f32)).ceil() as i32;
        let y1 = ((y + half).min(canvas.height as f32)).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let idx = ((py as u32 * canvas.width + px as u32) * 4) as usize;
                blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
            }
        }
    }
}

fn draw_rect_stroke(canvas: &mut Canvas, x: f32, y: f32, w: f32, h: f32, width: f32, color: Color) {
    // Top edge.
    canvas.fill_rect(x, y, w, width, color);
    // Bottom edge.
    canvas.fill_rect(x, y + h - width, w, width, color);
    // Left edge.
    canvas.fill_rect(x, y, width, h, color);
    // Right edge.
    canvas.fill_rect(x + w - width, y, width, h, color);
}

fn fill_polygon(canvas: &mut Canvas, pts: &[(f32, f32)], color: Color) {
    if pts.len() < 3 {
        return;
    }
    // Scanline fill.
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
        .min(canvas.height as f32)
        .ceil() as i32;
    for y in min_y..max_y {
        let mut intersections = Vec::new();
        let yc = y as f32 + 0.5;
        for i in 0..pts.len() {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % pts.len()];
            if (y1 <= yc && y2 > yc) || (y2 <= yc && y1 > yc) {
                let t = (yc - y1) / (y2 - y1);
                let x = x1 + t * (x2 - x1);
                intersections.push(x);
            }
        }
        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for chunk in intersections.chunks(2) {
            if chunk.len() == 2 {
                let x0 = chunk[0].max(0.0).floor() as i32;
                let x1 = chunk[1].min(canvas.width as f32).ceil() as i32;
                for x in x0..x1 {
                    let idx = ((y as u32 * canvas.width + x as u32) * 4) as usize;
                    blend_pixel(&mut canvas.pixels[idx..idx + 4], color);
                }
            }
        }
    }
}

fn stroke_path(
    canvas: &mut Canvas,
    commands: &[PathCommand],
    ox: f32,
    oy: f32,
    scale_x: f32,
    scale_y: f32,
    vb_x: f32,
    vb_y: f32,
    width: f32,
    color: Color,
) {
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;
    let mut start_x = 0.0f32;
    let mut start_y = 0.0f32;
    let to_screen =
        |x: f32, y: f32| -> (f32, f32) { (ox + (x - vb_x) * scale_x, oy + (y - vb_y) * scale_y) };
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(x, y) => {
                cur_x = *x;
                cur_y = *y;
                start_x = *x;
                start_y = *y;
            }
            PathCommand::LineTo(x, y) => {
                let (sx, sy) = to_screen(cur_x, cur_y);
                let (ex, ey) = to_screen(*x, *y);
                draw_line(canvas, sx, sy, ex, ey, width, color);
                cur_x = *x;
                cur_y = *y;
            }
            PathCommand::HorizontalLineTo(x) => {
                let (sx, sy) = to_screen(cur_x, cur_y);
                let (ex, ey) = to_screen(*x, cur_y);
                draw_line(canvas, sx, sy, ex, ey, width, color);
                cur_x = *x;
            }
            PathCommand::VerticalLineTo(y) => {
                let (sx, sy) = to_screen(cur_x, cur_y);
                let (ex, ey) = to_screen(cur_x, *y);
                draw_line(canvas, sx, sy, ex, ey, width, color);
                cur_y = *y;
            }
            PathCommand::ClosePath => {
                let (sx, sy) = to_screen(cur_x, cur_y);
                let (ex, ey) = to_screen(start_x, start_y);
                draw_line(canvas, sx, sy, ex, ey, width, color);
                cur_x = start_x;
                cur_y = start_y;
            }
            // Bezier curves — approximate with line segments.
            PathCommand::CubicBezier(x1, y1, x2, y2, x3, y3) => {
                let steps = 20;
                let (mut px, mut py) = (cur_x, cur_y);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let t2 = t * t;
                    let t3 = t2 * t;
                    let mt = 1.0 - t;
                    let mt2 = mt * mt;
                    let mt3 = mt2 * mt;
                    let nx = mt3 * cur_x + 3.0 * mt2 * t * x1 + 3.0 * mt * t2 * x2 + t3 * x3;
                    let ny = mt3 * cur_y + 3.0 * mt2 * t * y1 + 3.0 * mt * t2 * y2 + t3 * y3;
                    let (sx, sy) = to_screen(px, py);
                    let (ex, ey) = to_screen(nx, ny);
                    draw_line(canvas, sx, sy, ex, ey, width, color);
                    px = nx;
                    py = ny;
                }
                cur_x = *x3;
                cur_y = *y3;
            }
            PathCommand::QuadraticBezier(x1, y1, x2, y2) => {
                let steps = 20;
                let (mut px, mut py) = (cur_x, cur_y);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let nx = mt * mt * cur_x + 2.0 * mt * t * x1 + t * t * x2;
                    let ny = mt * mt * cur_y + 2.0 * mt * t * y1 + t * t * y2;
                    let (sx, sy) = to_screen(px, py);
                    let (ex, ey) = to_screen(nx, ny);
                    draw_line(canvas, sx, sy, ex, ey, width, color);
                    px = nx;
                    py = ny;
                }
                cur_x = *x2;
                cur_y = *y2;
            }
            _ => {}
        }
    }
}

fn path_to_points(
    commands: &[PathCommand],
    ox: f32,
    oy: f32,
    scale_x: f32,
    scale_y: f32,
    vb_x: f32,
    vb_y: f32,
) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    let to_screen =
        |x: f32, y: f32| -> (f32, f32) { (ox + (x - vb_x) * scale_x, oy + (y - vb_y) * scale_y) };
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(x, y) => {
                pts.push(to_screen(*x, *y));
            }
            PathCommand::LineTo(x, y) => {
                pts.push(to_screen(*x, *y));
            }
            PathCommand::HorizontalLineTo(x) => {
                // H/V commands are now converted to LineTo in the parser,
                // but handle them here too just in case.
                if let Some(&(_, last_y)) = pts.last() {
                    pts.push(to_screen(*x, last_y));
                }
            }
            PathCommand::VerticalLineTo(y) => {
                if let Some(&(last_x, _)) = pts.last() {
                    pts.push(to_screen(last_x, *y));
                }
            }
            PathCommand::ClosePath => {
                // ClosePath doesn't add a point — it's handled by fill_polygon.
            }
            // Bezier commands are now converted to LineTo segments in the parser,
            // but handle them as approximations here too.
            PathCommand::CubicBezier(x1, y1, x2, y2, x, y) => {
                if let Some(&(cx, cy)) = pts.last() {
                    for i in 1..=8 {
                        let t = i as f32 / 8.0;
                        let t2 = t * t;
                        let t3 = t2 * t;
                        let mt = 1.0 - t;
                        let mt2 = mt * mt;
                        let mt3 = mt2 * mt;
                        let px = mt3 * cx + 3.0 * mt2 * t * x1 + 3.0 * mt * t2 * x2 + t3 * x;
                        let py = mt3 * cy + 3.0 * mt2 * t * y1 + 3.0 * mt * t2 * y2 + t3 * y;
                        pts.push(to_screen(px, py));
                    }
                }
            }
            PathCommand::SmoothCubicBezier(x2, y2, x, y) => {
                if let Some(&(cx, cy)) = pts.last() {
                    for i in 1..=8 {
                        let t = i as f32 / 8.0;
                        let t2 = t * t;
                        let t3 = t2 * t;
                        let mt = 1.0 - t;
                        let mt2 = mt * mt;
                        let mt3 = mt2 * mt;
                        let px = mt3 * cx + 3.0 * mt2 * t * cx + 3.0 * mt * t2 * x2 + t3 * x;
                        let py = mt3 * cy + 3.0 * mt2 * t * cy + 3.0 * mt * t2 * y2 + t3 * y;
                        pts.push(to_screen(px, py));
                    }
                }
            }
            PathCommand::QuadraticBezier(x1, y1, x, y) => {
                if let Some(&(cx, cy)) = pts.last() {
                    for i in 1..=8 {
                        let t = i as f32 / 8.0;
                        let mt = 1.0 - t;
                        let px = mt * mt * cx + 2.0 * mt * t * x1 + t * t * x;
                        let py = mt * mt * cy + 2.0 * mt * t * y1 + t * t * y;
                        pts.push(to_screen(px, py));
                    }
                }
            }
            PathCommand::SmoothQuadraticBezier(x, y) => {
                pts.push(to_screen(*x, *y));
            }
            PathCommand::Arc(_, _, _, _, _, x, y) => {
                pts.push(to_screen(*x, *y));
            }
        }
    }
    pts
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
