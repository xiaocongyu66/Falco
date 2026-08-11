//! Float layout — `float: left/right` with text wrapping.
//!
//! Floats are removed from normal flow and positioned to the left or
//! right of their containing block. Subsequent inline content wraps
//! around them.

use crate::layout::{LayoutBox, LayoutContext, Rect};

/// A floated element with its bounds.
#[derive(Debug, Clone, Copy)]
pub struct FloatBox {
    pub rect: Rect,
    pub side: FloatSide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatSide {
    Left,
    Right,
}

/// Collect all floated children from a container, removing them from
/// the normal flow.
pub fn extract_floats(children: &mut Vec<LayoutBox>) -> Vec<(usize, FloatSide)> {
    let mut floats = Vec::new();
    for (i, child) in children.iter().enumerate() {
        if child.style.float != crate::style::Float::None {
            let side = match child.style.float {
                crate::style::Float::Left => FloatSide::Left,
                crate::style::Float::Right => FloatSide::Right,
                _ => FloatSide::Left,
            };
            floats.push((i, side));
        }
    }
    floats
}

/// Layout floated elements and return their bounds for text wrapping.
pub fn layout_floats(
    floats: &[(usize, FloatSide)],
    children: &mut [LayoutBox],
    origin_x: f32,
    origin_y: f32,
    available_width: f32,
    viewport_width: f32,
    container_font_size: f32,
) -> Vec<FloatBox> {
    let mut float_boxes = Vec::new();
    let mut left_x = origin_x;
    let mut right_x = origin_x + available_width;
    let mut current_y = origin_y;

    for &(idx, side) in floats {
        let child = &mut children[idx];
        let mut ctx = LayoutContext {
            viewport_width,
            x: if side == FloatSide::Left {
                left_x
            } else {
                right_x - 200.0
            },
            y: current_y,
            available_width: available_width / 2.0, // Floats take up to half width
            containing_font_size: container_font_size,
        };
        crate::layout::layout_block_pub(child, &mut ctx);

        let fb = FloatBox {
            rect: child.bounds,
            side,
        };

        match side {
            FloatSide::Left => {
                left_x = fb.rect.x + fb.rect.width + 8.0;
            }
            FloatSide::Right => {
                right_x = fb.rect.x;
            }
        }

        if fb.rect.y + fb.rect.height > current_y {
            current_y = fb.rect.y + fb.rect.height + 8.0;
        }

        float_boxes.push(fb);
    }

    float_boxes
}

/// Calculate the available width at a given Y position, accounting for
/// floats that intrude into that line.
pub fn available_width_at_y(
    y: f32,
    float_boxes: &[FloatBox],
    full_width: f32,
    origin_x: f32,
) -> (f32, f32) {
    let mut left_edge = origin_x;
    let mut right_edge = origin_x + full_width;

    for fb in float_boxes {
        // Check if this float intrudes at this Y position.
        if y >= fb.rect.y && y < fb.rect.y + fb.rect.height {
            match fb.side {
                FloatSide::Left => {
                    left_edge = left_edge.max(fb.rect.x + fb.rect.width);
                }
                FloatSide::Right => {
                    right_edge = right_edge.min(fb.rect.x);
                }
            }
        }
    }

    (left_edge, right_edge)
}

/// Apply `clear: left/right/both` — moves Y past any floats.
pub fn apply_clear(y: &mut f32, clear: crate::style::Clear, float_boxes: &[FloatBox]) {
    if clear == crate::style::Clear::None {
        return;
    }
    for fb in float_boxes {
        let should_clear = match clear {
            crate::style::Clear::Left => fb.side == FloatSide::Left,
            crate::style::Clear::Right => fb.side == FloatSide::Right,
            crate::style::Clear::Both => true,
            _ => false,
        };
        if should_clear {
            let bottom = fb.rect.y + fb.rect.height;
            if bottom > *y {
                *y = bottom + 8.0;
            }
        }
    }
}
