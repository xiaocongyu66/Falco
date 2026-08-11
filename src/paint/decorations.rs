//! Text decorations — underline, line-through, overline.
//! Also handles list bullet rendering and text-transform application.

use crate::css::Color;
use crate::paint::PaintCommand;
use crate::style::extra::{
    apply_text_transform, list_bullet, ListStyleType, TextDecoration, TextTransform,
};

/// Emit text-decoration paint commands for a text element.
/// Draws a line under (underline), through (line-through), or over (overline) the text.
pub fn emit_text_decoration(
    x: f32,
    y: f32,
    w: f32,
    font_size: f32,
    decoration: TextDecoration,
    color: Color,
    out: &mut Vec<PaintCommand>,
) {
    if decoration == TextDecoration::None {
        return;
    }
    let line_y = match decoration {
        TextDecoration::Underline => y + font_size,
        TextDecoration::LineThrough => y + font_size * 0.5,
        TextDecoration::Overline => y,
        TextDecoration::None => return,
    };
    out.push(PaintCommand::RoundedRect {
        x,
        y: line_y,
        w,
        h: 1.0,
        color,
        radius: Default::default(),
    });
}

/// Emit a list bullet marker for a `<li>` element.
/// Draws the bullet at the left padding area of the list item.
pub fn emit_list_bullet(
    x: f32,
    y: f32,
    font_size: f32,
    style: ListStyleType,
    index: usize,
    color: Color,
    out: &mut Vec<PaintCommand>,
) {
    if style == ListStyleType::None {
        return;
    }
    let bullet = list_bullet(style, index);
    if bullet.is_empty() {
        return;
    }
    out.push(PaintCommand::Text {
        x,
        y,
        text: bullet,
        color,
        font_size: font_size * 0.8,
        font_weight: 400,
        italic: false,
        family: "sans-serif".to_string(),
    });
}

/// Apply text-transform to a string before rendering.
pub fn transform_text(text: &str, transform: TextTransform) -> String {
    apply_text_transform(text, transform)
}
