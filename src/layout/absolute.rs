//! Position: absolute/fixed layout.
//!
//! Elements with `position: absolute` are removed from normal flow and
//! positioned relative to their nearest positioned ancestor (or the
//! initial containing block if none exists).
//!
//! `position: fixed` is treated the same as `absolute` but relative to
//! the viewport (which in Falco's case is the same thing).

use crate::layout::{BoxContent, LayoutBox, Length};
use crate::style::Position;

/// Check if an element is positioned (relative, absolute, or fixed).
pub fn is_positioned(box_node: &LayoutBox) -> bool {
    box_node.style.position != Position::Static
}

/// Resolve absolute positioning offsets for a box.
/// Returns (x, y) offsets relative to the containing block.
pub fn resolve_absolute_position(
    box_node: &LayoutBox,
    containing_block_x: f32,
    containing_block_y: f32,
    containing_block_width: f32,
    containing_block_height: f32,
) -> (f32, f32) {
    let style = &box_node.style;

    // Resolve left/top offsets.
    let left = match style.left {
        Some(Length::Px(v)) => v,
        Some(Length::Percent(p)) => containing_block_width * p / 100.0,
        _ => 0.0,
    };
    let top = match style.top {
        Some(Length::Px(v)) => v,
        Some(Length::Percent(p)) => containing_block_height * p / 100.0,
        _ => 0.0,
    };

    // If right is set and left is not, compute from right.
    if style.left.is_none() {
        if let Some(Length::Px(right)) = style.right {
            let _ = right; // Would need element width to compute properly.
        }
    }

    (containing_block_x + left, containing_block_y + top)
}

/// Apply relative positioning offset to a box's bounds.
pub fn apply_relative_offset(box_node: &mut LayoutBox) {
    if box_node.style.position != Position::Relative {
        return;
    }
    let style = &box_node.style;

    let dx = match style.left {
        Some(Length::Px(v)) => v,
        Some(Length::Percent(_)) => 0.0, // Would need parent width.
        _ => match style.right {
            Some(Length::Px(v)) => -v,
            _ => 0.0,
        },
    };

    let dy = match style.top {
        Some(Length::Px(v)) => v,
        Some(Length::Percent(_)) => 0.0,
        _ => match style.bottom {
            Some(Length::Px(v)) => -v,
            _ => 0.0,
        },
    };

    box_node.bounds.x += dx;
    box_node.bounds.y += dy;
}

/// Walk the layout tree and apply relative positioning offsets.
/// This is called after the initial layout pass.
pub fn apply_all_relative_offsets(root: &mut LayoutBox) {
    apply_relative_offset(root);
    match &mut root.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children.iter_mut() {
                apply_all_relative_offsets(c);
            }
        }
        BoxContent::Inline(items) => {
            for item in items.iter_mut() {
                if let crate::layout::InlineItem::Text(child) = item {
                    apply_all_relative_offsets(child);
                }
            }
        }
        _ => {}
    }
}
