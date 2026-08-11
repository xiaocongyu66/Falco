//! CSS Grid layout — `display: grid` with template columns/rows.
//!
//! Supports:
//!   - `grid-template-columns: 1fr 2fr 100px auto` (fr units, fixed, auto)
//!   - `grid-template-rows: same syntax`
//!   - `gap` / `column-gap` / `row-gap`
//!   - `grid-column: 1 / 3` (start / end line)
//!   - `grid-row: 1 / 2`
//!   - `justify-items` / `align-items` per cell
//!   - Auto-placement for items without explicit grid-area

use crate::layout::{LayoutBox, LayoutContext};

/// A grid track sizing (column or row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Fr(f32),      // Fractional unit (1fr = 1 part of available space)
    Px(f32),      // Fixed pixel size
    Auto,         // Sized to content
    MinContent,   // Minimum content size
    MaxContent,   // Maximum content size
    Percent(f32), // Percentage of container size
}

/// A grid item's placement.
#[derive(Debug, Clone, Copy, Default)]
pub struct GridPlacement {
    pub column_start: Option<usize>,
    pub column_end: Option<usize>,
    pub row_start: Option<usize>,
    pub row_end: Option<usize>,
}

/// Parse a grid-template-columns/rows value string into track sizes.
/// E.g. "1fr 2fr 100px auto" → [Fr(1), Fr(2), Px(100), Auto]
pub fn parse_track_sizes(value: &str) -> Vec<TrackSize> {
    value
        .split_whitespace()
        .map(|part| {
            let part = part.trim();
            if part.ends_with("fr") {
                let n = part.trim_end_matches("fr").parse::<f32>().unwrap_or(1.0);
                TrackSize::Fr(n)
            } else if part.ends_with("px") {
                let n = part.trim_end_matches("px").parse::<f32>().unwrap_or(0.0);
                TrackSize::Px(n)
            } else if part.ends_with("%") {
                let n = part.trim_end_matches("%").parse::<f32>().unwrap_or(0.0);
                TrackSize::Percent(n)
            } else if part == "auto" {
                TrackSize::Auto
            } else if part == "min-content" {
                TrackSize::MinContent
            } else if part == "max-content" {
                TrackSize::MaxContent
            } else if let Ok(n) = part.parse::<f32>() {
                TrackSize::Px(n)
            } else {
                TrackSize::Auto
            }
        })
        .collect()
}

/// Parse grid-column / grid-row shorthand "1 / 3" into (start, end).
pub fn parse_grid_line(value: &str) -> (Option<usize>, Option<usize>) {
    let parts: Vec<&str> = value.split('/').map(|s| s.trim()).collect();
    let start = parts.first().and_then(|s| s.parse::<usize>().ok());
    let end = parts.get(1).and_then(|s| s.parse::<usize>().ok());
    (start, end)
}

/// Layout a grid container's children.
pub fn layout_grid(
    children: &mut [LayoutBox],
    origin_x: f32,
    origin_y: f32,
    available_width: f32,
    viewport_width: f32,
    container_font_size: f32,
    container_style: &crate::style::ComputedStyle,
) {
    // Parse grid-template-columns from the property store.
    let template_cols = container_style
        .property_store
        .get("grid-template-columns")
        .map(|s| parse_track_sizes(s))
        .unwrap_or_default();

    let template_rows = container_style
        .property_store
        .get("grid-template-rows")
        .map(|s| parse_track_sizes(s))
        .unwrap_or_default();

    let gap = container_style.gap;
    let col_gap = container_style
        .property_store
        .get("column-gap")
        .and_then(|s| s.trim_end_matches("px").parse().ok())
        .unwrap_or(gap);
    let row_gap = container_style
        .property_store
        .get("row-gap")
        .and_then(|s| s.trim_end_matches("px").parse().ok())
        .unwrap_or(gap);

    // If no template columns, auto-create equal columns.
    let cols = if template_cols.is_empty() {
        // Auto: 1 column per item, stacked vertically.
        layout_grid_auto_vertical(
            children,
            origin_x,
            origin_y,
            available_width,
            row_gap,
            viewport_width,
            container_font_size,
        );
        return;
    } else {
        template_cols.len()
    };

    // Calculate column widths.
    let total_gap = col_gap * (cols as f32 - 1.0).max(0.0);
    let available_for_cols = available_width - total_gap;

    // Calculate fr total.
    let total_fr: f32 = template_cols
        .iter()
        .filter_map(|t| {
            if let TrackSize::Fr(n) = t {
                Some(*n)
            } else {
                None
            }
        })
        .sum();

    let fr_space = if total_fr > 0.0 {
        available_for_cols
    } else {
        0.0
    };

    let mut col_widths: Vec<f32> = template_cols
        .iter()
        .map(|t| {
            match t {
                TrackSize::Fr(n) => {
                    if total_fr > 0.0 {
                        fr_space * (n / total_fr)
                    } else {
                        0.0
                    }
                }
                TrackSize::Px(n) => *n,
                TrackSize::Percent(p) => available_for_cols * p / 100.0,
                TrackSize::Auto => 0.0, // Calculated after content measurement
                TrackSize::MinContent => 0.0,
                TrackSize::MaxContent => 0.0,
            }
        })
        .collect();

    // Distribute remaining space to Auto columns.
    let used_space: f32 = col_widths.iter().sum();
    let auto_cols: Vec<usize> = template_cols
        .iter()
        .enumerate()
        .filter(|(_, t)| **t == TrackSize::Auto)
        .map(|(i, _)| i)
        .collect();
    if !auto_cols.is_empty() {
        let auto_space = (available_for_cols - used_space).max(0.0) / auto_cols.len() as f32;
        for &i in &auto_cols {
            col_widths[i] = auto_space;
        }
    }

    // Calculate number of rows needed.
    let n = children.len();
    let rows = if template_rows.is_empty() {
        (n + cols - 1) / cols.max(1) // ceiling division
    } else {
        template_rows.len()
    };

    // Calculate row heights (auto for now — sized to content).
    let mut row_heights: Vec<f32> = if template_rows.is_empty() {
        vec![0.0; rows]
    } else {
        template_rows
            .iter()
            .map(|t| {
                match t {
                    TrackSize::Fr(_) => 0.0,
                    TrackSize::Px(n) => *n,
                    TrackSize::Percent(p) => 100.0 * p / 100.0, // Approximate
                    _ => 0.0,
                }
            })
            .collect()
    };

    // Place children in the grid.
    let mut col = 0;
    let mut row = 0;
    let max_row_height = 0.0f32;

    for (i, child) in children.iter_mut().enumerate() {
        // Check for explicit grid placement.
        let (col_start, col_end) = child
            .style
            .property_store
            .get("grid-column")
            .map(|s| parse_grid_line(s))
            .unwrap_or((None, None));
        let (row_start, _row_end) = child
            .style
            .property_store
            .get("grid-row")
            .map(|s| parse_grid_line(s))
            .unwrap_or((None, None));

        let (cell_col, cell_row) = if let (Some(cs), Some(rs)) = (col_start, row_start) {
            (cs - 1, rs - 1) // CSS grid lines are 1-indexed
        } else if col_start.is_some() {
            (col_start.unwrap() - 1, row)
        } else {
            let c = col;
            let r = row;
            col += 1;
            if col >= cols {
                col = 0;
                row += 1;
            }
            (c, r)
        };

        // Ensure row_heights has enough entries.
        while row_heights.len() <= cell_row {
            row_heights.push(0.0);
        }

        // Calculate cell position.
        let mut cell_x = origin_x;
        for c in 0..cell_col.min(cols) {
            cell_x += col_widths[c] + col_gap;
        }
        let mut cell_y = origin_y;
        for r in 0..cell_row.min(row_heights.len()) {
            cell_y += row_heights[r] + row_gap;
        }

        // Calculate cell span.
        let span = col_end.map(|e| e.saturating_sub(cell_col + 1)).unwrap_or(0);
        let span_width: f32 = (0..=span)
            .map(|s| col_widths.get(cell_col + s).copied().unwrap_or(0.0))
            .sum::<f32>()
            + col_gap * span as f32;

        // Layout the child.
        let mut child_ctx = LayoutContext {
            viewport_width,
            x: cell_x,
            y: cell_y,
            available_width: span_width
                .max(col_widths.get(cell_col).copied().unwrap_or(available_width)),
            containing_font_size: container_font_size,
        };
        crate::layout::layout_block_pub(child, &mut child_ctx);

        // Update row height.
        let child_h = child.bounds.height;
        if child_h > row_heights[cell_row] {
            row_heights[cell_row] = child_h;
        }

        let _ = i;
        let _ = max_row_height;
    }

    // Apply justify-items / align-items.
    let justify_items = container_style
        .property_store
        .get("justify-items")
        .map(|s| s.as_str())
        .unwrap_or("stretch");
    let align_items = container_style
        .property_store
        .get("align-items")
        .map(|s| s.as_str())
        .unwrap_or("stretch");

    // Re-position children based on alignment.
    let mut col = 0;
    let mut row = 0;
    for child in children.iter_mut() {
        let cell_col = col;
        let cell_row = row;

        let mut cell_x = origin_x;
        for c in 0..cell_col.min(cols) {
            cell_x += col_widths[c] + col_gap;
        }
        let mut cell_y = origin_y;
        for r in 0..cell_row.min(row_heights.len()) {
            cell_y += row_heights[r] + row_gap;
        }

        let cell_w = col_widths
            .get(cell_col)
            .copied()
            .unwrap_or(available_width / cols as f32);
        let cell_h = row_heights.get(cell_row).copied().unwrap_or(0.0);

        match justify_items {
            "center" => child.bounds.x = cell_x + (cell_w - child.bounds.width) / 2.0,
            "end" | "right" => child.bounds.x = cell_x + cell_w - child.bounds.width,
            _ => child.bounds.x = cell_x, // start / stretch
        }
        match align_items {
            "center" => child.bounds.y = cell_y + (cell_h - child.bounds.height) / 2.0,
            "end" | "bottom" => child.bounds.y = cell_y + cell_h - child.bounds.height,
            _ => child.bounds.y = cell_y, // start / stretch
        }

        col += 1;
        if col >= cols {
            col = 0;
            row += 1;
        }
    }
}

/// Auto-layout for grid with no template (stacks vertically).
fn layout_grid_auto_vertical(
    children: &mut [LayoutBox],
    origin_x: f32,
    origin_y: f32,
    available_width: f32,
    row_gap: f32,
    viewport_width: f32,
    container_font_size: f32,
) {
    let mut y = origin_y;
    for child in children.iter_mut() {
        let mut ctx = LayoutContext {
            viewport_width,
            x: origin_x,
            y,
            available_width,
            containing_font_size: container_font_size,
        };
        crate::layout::layout_block_pub(child, &mut ctx);
        y = child.bounds.y + child.bounds.height + row_gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_sizes() {
        let tracks = parse_track_sizes("1fr 2fr 100px auto");
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0], TrackSize::Fr(1.0));
        assert_eq!(tracks[1], TrackSize::Fr(2.0));
        assert_eq!(tracks[2], TrackSize::Px(100.0));
        assert_eq!(tracks[3], TrackSize::Auto);
    }

    #[test]
    fn parses_grid_line() {
        let (start, end) = parse_grid_line("1 / 3");
        assert_eq!(start, Some(1));
        assert_eq!(end, Some(3));
    }
}
