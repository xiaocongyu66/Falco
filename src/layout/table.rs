//! CSS table layout — renders `<table>`, `<thead>`, `<tbody>`, `<tr>`, `<td>`, `<th>`.
//!
//! Implements a simplified table layout:
//! - Each `<tr>` is a horizontal row
//! - Each `<td>`/`<th>` is a cell in the row
//! - Column widths are distributed evenly across available width
//! - Cell padding and borders are respected

use crate::css::Color;
use crate::layout::{BoxContent, InlineItem, LayoutBox};
use crate::paint::PaintCommand;

/// Layout a table element. Detects `<table>` tags and delegates to
/// `layout_table`. Returns true if the element was handled as a table.
pub fn is_table_element(box_node: &LayoutBox) -> bool {
    box_node.tag.as_deref() == Some("table")
}

/// Check if a layout box is a table-related element.
pub fn is_table_part(box_node: &LayoutBox) -> bool {
    matches!(
        box_node.tag.as_deref(),
        Some("table")
            | Some("thead")
            | Some("tbody")
            | Some("tfoot")
            | Some("tr")
            | Some("td")
            | Some("th")
            | Some("caption")
            | Some("colgroup")
            | Some("col")
    )
}

/// Collect all `<td>` and `<th>` cells from a `<tr>`, recursing into
/// inline content as needed.
pub fn collect_table_cells(row: &LayoutBox) -> Vec<&LayoutBox> {
    let mut cells = Vec::new();
    collect_cells_recursive(row, &mut cells);
    cells
}

fn collect_cells_recursive<'a>(box_node: &'a LayoutBox, cells: &mut Vec<&'a LayoutBox>) {
    match &box_node.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                if c.tag.as_deref() == Some("td") || c.tag.as_deref() == Some("th") {
                    cells.push(c);
                } else {
                    collect_cells_recursive(c, cells);
                }
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    collect_cells_recursive(child, cells);
                }
            }
        }
        _ => {}
    }
}

/// Collect all `<tr>` rows from a table, recursing into thead/tbody/tfoot.
pub fn collect_table_rows(table: &LayoutBox) -> Vec<&LayoutBox> {
    let mut rows = Vec::new();
    collect_rows_recursive(table, &mut rows);
    rows
}

fn collect_rows_recursive<'a>(box_node: &'a LayoutBox, rows: &mut Vec<&'a LayoutBox>) {
    match &box_node.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                if c.tag.as_deref() == Some("tr") {
                    rows.push(c);
                } else {
                    collect_rows_recursive(c, rows);
                }
            }
        }
        _ => {}
    }
}

/// Paint a table: draws cell backgrounds, borders, and recurses into cell content.
pub fn paint_table(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let style = &box_node.style;

    // Draw table background.
    if style.background_color.a > 0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x,
            y: b.y,
            w: b.width,
            h: b.height,
            color: style.background_color,
            radius: style.border_radius,
        });
    }

    // Draw table border.
    let bw = crate::layout::border_widths_pub(&style.border);
    if bw.top > 0.0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x,
            y: b.y,
            w: b.width,
            h: bw.top,
            color: style.border.top.color,
            radius: Default::default(),
        });
    }
    if bw.bottom > 0.0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x,
            y: b.y + b.height - bw.bottom,
            w: b.width,
            h: bw.bottom,
            color: style.border.bottom.color,
            radius: Default::default(),
        });
    }
    if bw.left > 0.0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x,
            y: b.y,
            w: bw.left,
            h: b.height,
            color: style.border.left.color,
            radius: Default::default(),
        });
    }
    if bw.right > 0.0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x + b.width - bw.right,
            y: b.y,
            w: bw.right,
            h: b.height,
            color: style.border.right.color,
            radius: Default::default(),
        });
    }

    // Recurse into children — cells are painted by the normal paint_box.
    match &box_node.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                crate::layout::paint_box_pub(c, out);
            }
        }
        _ => {}
    }
}

/// Paint a table cell: draws cell background, borders, then cell content.
pub fn paint_table_cell(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let style = &box_node.style;

    // Cell background.
    if style.background_color.a > 0 {
        out.push(PaintCommand::RoundedRect {
            x: b.x,
            y: b.y,
            w: b.width,
            h: b.height,
            color: style.background_color,
            radius: Default::default(),
        });
    }

    // Cell borders (1px grey by default).
    let border_color =
        if style.border.top.color != Color::TRANSPARENT && style.border.top.width > 0.0 {
            style.border.top.color
        } else {
            Color::rgb(200, 200, 200)
        };
    let border_w = if style.border.top.width > 0.0 {
        style.border.top.width
    } else {
        1.0
    };

    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: b.width,
        h: border_w,
        color: border_color,
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y + b.height - border_w,
        w: b.width,
        h: border_w,
        color: border_color,
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: border_w,
        h: b.height,
        color: border_color,
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x + b.width - border_w,
        y: b.y,
        w: border_w,
        h: b.height,
        color: border_color,
        radius: Default::default(),
    });

    // Cell content — recurse.
    match &box_node.content {
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                crate::layout::paint_box_pub(c, out);
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    crate::layout::paint_box_pub(child, out);
                }
            }
        }
        BoxContent::Text {
            text,
            color,
            font_size,
            font_weight,
            font_style,
            font_family,
        } if !text.is_empty() => {
            out.push(PaintCommand::Text {
                x: b.x,
                y: b.y,
                text: text.clone(),
                color: *color,
                font_size: *font_size,
                font_weight: *font_weight,
                italic: *font_style == crate::style::FontStyle::Italic,
                family: font_family.clone(),
            });
        }
        _ => {}
    }
}
