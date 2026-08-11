//! Layout engine — block + inline + flexbox + table layout.
//!
//! Computes positions and dimensions for every visible element.

pub mod absolute;
pub mod float;
pub mod grid;
pub mod table;

use crate::dom::Node;
use crate::paint::PaintCommand;
use crate::style::{BorderEdges, ComputedStyle, Display, EdgeSizes, Length, StyleTree};
use std::collections::HashMap;

/// A laid-out box. Modeled after the CSS box tree but kept simple.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub style: ComputedStyle,
    pub bounds: Rect,
    pub content: BoxContent,
    /// Tag name of the DOM element this box was built from (e.g. "a", "input", "button").
    /// `None` for anonymous boxes (text nodes, document root).
    pub tag: Option<String>,
    /// Attributes of the DOM element this box was built from.
    /// Used for hit-testing links (`href`), inputs (`type`, `name`, `value`), etc.
    pub attrs: HashMap<String, String>,
    /// Unique id assigned during layout — used for focus tracking.
    pub element_id: Option<usize>,
}

impl LayoutBox {
    /// True if this box is interactive (link, input, button, textarea).
    pub fn is_interactive(&self) -> bool {
        match self.tag.as_deref() {
            Some("a") => self.attrs.contains_key("href"),
            Some("input") | Some("button") | Some("textarea") | Some("select") => true,
            _ => false,
        }
    }

    /// Returns the href if this is a link.
    pub fn href(&self) -> Option<&str> {
        if self.tag.as_deref() == Some("a") {
            self.attrs.get("href").map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Returns the input type ("text", "password", "submit", "button", ...).
    pub fn input_type(&self) -> Option<&str> {
        if self.tag.as_deref() == Some("input") {
            Some(self.attrs.get("type").map(|s| s.as_str()).unwrap_or("text"))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum BoxContent {
    /// A block-level container — children stack vertically.
    Block(Vec<LayoutBox>),
    /// A flex container — children are flex items.
    Flex(Vec<LayoutBox>),
    /// A grid container — children are grid items.
    Grid(Vec<LayoutBox>),
    /// A table container — children are rows.
    Table(Vec<LayoutBox>),
    /// An inline container — children flow horizontally and wrap.
    Inline(Vec<InlineItem>),
    /// A leaf — just text.
    Text {
        text: String,
        color: crate::css::Color,
        font_size: f32,
        font_weight: u16,
        font_style: crate::style::FontStyle,
        font_family: String,
    },
    /// An empty placeholder (e.g. `<br>`).
    Empty,
}

#[derive(Debug, Clone)]
pub enum InlineItem {
    Text(LayoutBox),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }
}

/// Build the layout tree from the style tree, then compute positions
/// starting at the viewport root.
pub fn build_layout_tree<'a>(style_root: &'a StyleTree<'a>, viewport_width: f32) -> LayoutBox {
    let mut root = build_box(style_root);
    // Initial layout: position at origin, full width.
    let mut ctx = LayoutContext {
        viewport_width,
        x: 0.0,
        y: 0.0,
        available_width: viewport_width,
        containing_font_size: style_root.style.font_size,
    };
    layout_block(&mut root, &mut ctx);
    root
}

fn build_box(style_node: &StyleTree) -> LayoutBox {
    let style = style_node.style.clone();
    // Extract tag and attrs from the DOM element.
    let (tag, attrs) = match style_node.node {
        Node::Element(e) => (Some(e.tag.clone()), e.attrs.clone()),
        _ => (None, HashMap::new()),
    };
    if let Some(ref t) = tag {}
    let content = match style_node.node {
        Node::Element(_) | Node::Document(_) => {
            if style.display == Display::None {
                BoxContent::Empty
            } else if style.display == Display::Inline || style.display == Display::InlineBlock {
                let mut items: Vec<InlineItem> = Vec::new();
                for child in &style_node.children {
                    collect_inline_items(child, &mut items);
                }
                BoxContent::Inline(items)
            } else if style.display == Display::Flex || style.display == Display::InlineFlex {
                let children: Vec<LayoutBox> = style_node
                    .children
                    .iter()
                    .filter(|c| c.style.display != Display::None)
                    .filter(|c| match c.node {
                        Node::Text(t) => !t.text.trim().is_empty(),
                        _ => true,
                    })
                    .map(build_box)
                    .collect();
                BoxContent::Flex(children)
            } else if style.display == Display::Grid
                || style.display == Display::InlineGrid
                || style.property_store.contains_key("grid-template-columns")
                || style.property_store.contains_key("grid-template-rows")
            {
                // Grid container — detected by grid-template-* properties.
                let children: Vec<LayoutBox> = style_node
                    .children
                    .iter()
                    .filter(|c| c.style.display != Display::None)
                    .filter(|c| match c.node {
                        Node::Text(t) => !t.text.trim().is_empty(),
                        _ => true,
                    })
                    .map(build_box)
                    .collect();
                BoxContent::Grid(children)
            } else if style.display == Display::Table
                || style.display == Display::TableRow
                || style.display == Display::TableCell
                || style.display == Display::TableRowGroup
                || style.display == Display::TableHeaderGroup
                || style.display == Display::TableFooterGroup
            {
                // Table layout.
                let children: Vec<LayoutBox> = style_node
                    .children
                    .iter()
                    .filter(|c| c.style.display != Display::None)
                    .filter(|c| match c.node {
                        Node::Text(t) => !t.text.trim().is_empty(),
                        _ => true,
                    })
                    .map(build_box)
                    .collect();
                BoxContent::Table(children)
            } else {
                let children: Vec<LayoutBox> = style_node
                    .children
                    .iter()
                    .filter(|c| c.style.display != Display::None)
                    .filter(|c| match c.node {
                        Node::Text(t) => !t.text.trim().is_empty(),
                        _ => true,
                    })
                    .map(build_box)
                    .collect();
                BoxContent::Block(children)
            }
        }
        Node::Text(t) => BoxContent::Text {
            text: t.text.clone(),
            color: style.color,
            font_size: style.font_size,
            font_weight: style.font_weight,
            font_style: style.font_style,
            font_family: style.font_family.clone(),
        },
        _ => BoxContent::Empty,
    };
    LayoutBox {
        style,
        bounds: Rect::default(),
        content,
        tag,
        attrs,
        element_id: None,
    }
}

fn collect_inline_items(style_node: &StyleTree, items: &mut Vec<InlineItem>) {
    match style_node.node {
        Node::Text(t) => {
            if !t.text.is_empty() {
                items.push(InlineItem::Text(LayoutBox {
                    style: style_node.style.clone(),
                    bounds: Rect::default(),
                    content: BoxContent::Text {
                        text: t.text.clone(),
                        color: style_node.style.color,
                        font_size: style_node.style.font_size,
                        font_weight: style_node.style.font_weight,
                        font_style: style_node.style.font_style,
                        font_family: style_node.style.font_family.clone(),
                    },
                    tag: None,
                    attrs: HashMap::new(),
                    element_id: None,
                }));
            }
        }
        Node::Element(_) => {
            if style_node.style.display == Display::None {
                return;
            }
            // For inline elements, recurse into children. For block elements
            // inside inline context (rare), we still recurse — the layout
            // engine will treat them as opaque text-width items.
            for child in &style_node.children {
                collect_inline_items(child, items);
            }
        }
        _ => {}
    }
}

struct LayoutContext {
    viewport_width: f32,
    x: f32,
    y: f32,
    available_width: f32,
    containing_font_size: f32,
}

/// Public layout_block — used by grid.rs and float.rs.
pub fn layout_block_pub(box_node: &mut LayoutBox, ctx: &mut LayoutContext) {
    layout_block(box_node, ctx);
}

fn layout_block(box_node: &mut LayoutBox, ctx: &mut LayoutContext) {
    let ctx = LayoutContext {
        viewport_width: ctx.viewport_width,
        x: ctx.x,
        y: ctx.y,
        available_width: ctx.available_width,
        containing_font_size: ctx.containing_font_size,
    };

    // Resolve width.
    let padding = box_node.style.padding;
    let border = border_widths(&box_node.style.border);
    let margin = box_node.style.margin;
    let is_border_box = box_node.style.box_sizing == crate::style::BoxSizing::BorderBox;

    let inner_width = match box_node.style.width {
        Some(Length::Px(w)) => {
            if is_border_box {
                // width includes padding + border.
                (w - padding.left - padding.right - border.left - border.right).max(0.0)
            } else {
                w
            }
        }
        Some(Length::Percent(p)) => {
            let w = ctx.available_width * p / 100.0;
            if is_border_box {
                (w - padding.left - padding.right - border.left - border.right).max(0.0)
            } else {
                w
            }
        }
        Some(Length::Em(em)) => em * box_node.style.font_size,
        Some(Length::Vw(v)) => v * ctx.viewport_width / 100.0,
        Some(Length::Vh(_)) | Some(Length::Auto) | None => (ctx.available_width
            - margin.left
            - margin.right
            - border.left
            - border.right
            - padding.left
            - padding.right)
            .max(0.0),
    };

    let outer_width = inner_width
        + padding.left
        + padding.right
        + border.left
        + border.right
        + margin.left
        + margin.right;

    box_node.bounds.x = ctx.x + margin.left;
    box_node.bounds.y = ctx.y + margin.top;
    box_node.bounds.width = inner_width + padding.left + padding.right + border.left + border.right;

    // Layout children — block flow.
    let mut child_y = box_node.bounds.y + border.top + padding.top;
    let child_x = box_node.bounds.x + border.left + padding.left;
    let child_width = inner_width;

    match &mut box_node.content {
        BoxContent::Block(children) => {
            for child in children.iter_mut() {
                let mut child_ctx = LayoutContext {
                    viewport_width: ctx.viewport_width,
                    x: child_x,
                    y: child_y,
                    available_width: child_width,
                    containing_font_size: box_node.style.font_size,
                };
                layout_block(child, &mut child_ctx);
                child_y = child.bounds.y + child.bounds.height + child.style.margin.bottom;
            }
        }
        BoxContent::Flex(children) => {
            // Extract style values before borrowing children mutably.
            let flex_direction = box_node.style.flex_direction;
            let justify_content = box_node.style.justify_content;
            let align_items = box_node.style.align_items;
            let gap = box_node.style.gap;
            let container_height = box_node.style.height;
            let container_padding_top = box_node.style.padding.top;
            let container_padding_bottom = box_node.style.padding.bottom;
            let container_font_size = box_node.style.font_size;
            layout_flex_impl(
                children,
                child_x,
                child_y,
                child_width,
                ctx.viewport_width,
                flex_direction,
                justify_content,
                align_items,
                gap,
                container_height,
                container_padding_top,
                container_padding_bottom,
                container_font_size,
            );
            let max_bottom = children
                .iter()
                .map(|c| c.bounds.y + c.bounds.height)
                .fold(child_y, f32::max);
            child_y = max_bottom;
        }
        BoxContent::Grid(children) => {
            let container_font_size = box_node.style.font_size;
            let viewport_w = ctx.viewport_width;
            grid::layout_grid(
                children,
                child_x,
                child_y,
                child_width,
                viewport_w,
                container_font_size,
                &box_node.style,
            );
            let max_bottom = children
                .iter()
                .map(|c| c.bounds.y + c.bounds.height)
                .fold(child_y, f32::max);
            child_y = max_bottom;
        }
        BoxContent::Table(children) => {
            // Simple table layout: stack rows vertically, cells horizontally.
            let mut row_y = child_y;
            let padding = 4.0;
            for row in children.iter_mut() {
                let mut cell_x = child_x;
                let mut row_height = 0.0f32;
                let cells_len = match &row.content {
                    BoxContent::Block(cells) | BoxContent::Table(cells) => cells.len(),
                    _ => 0,
                };
                if cells_len > 0 {
                    let cell_width = child_width / cells_len as f32;
                    if let BoxContent::Block(cells) | BoxContent::Table(cells) = &mut row.content {
                        for cell in cells.iter_mut() {
                            cell.bounds.x = cell_x + padding;
                            cell.bounds.y = row_y + padding;
                            cell.bounds.width = cell_width - 2.0 * padding;
                            cell.bounds.height = 20.0;
                            let mut cell_ctx = LayoutContext {
                                viewport_width: ctx.viewport_width,
                                x: cell.bounds.x,
                                y: cell.bounds.y,
                                available_width: cell.bounds.width,
                                containing_font_size: 16.0,
                            };
                            layout_block_pub(cell, &mut cell_ctx);
                            row_height = row_height.max(cell.bounds.height + 2.0 * padding);
                            cell_x += cell_width;
                        }
                    }
                }
                row.bounds.x = child_x;
                row.bounds.y = row_y;
                row.bounds.width = child_width;
                row.bounds.height = row_height;
                row_y += row_height;
            }
            child_y = row_y;
        }
        BoxContent::Inline(items) => {
            // Lay out inline items using a simple line-breaking algorithm.
            let line_height = box_node.style.line_height * box_node.style.font_size;
            let mut x = child_x;
            let mut y = child_y;
            let mut line_max_ascent = line_height;
            let wrap_width = child_width;

            for item in items.iter_mut() {
                if let InlineItem::Text(child_box) = item {
                    if let BoxContent::Text {
                        text, font_size, ..
                    } = &child_box.content
                    {
                        let font_size = *font_size;
                        let text = text.clone();
                        let words: Vec<&str> = text.split(' ').collect();
                        let space_width = measure_char(' ', font_size) * 0.3;
                        let mut current_x = x;
                        for (i, word) in words.iter().enumerate() {
                            let word_width = measure_text(word, font_size);
                            if i > 0 {
                                if current_x + space_width + word_width > child_x + wrap_width
                                    && current_x > child_x
                                {
                                    y += line_max_ascent;
                                    current_x = child_x;
                                    line_max_ascent = line_height;
                                } else {
                                    current_x += space_width;
                                }
                            }
                            child_box.bounds.x = current_x;
                            child_box.bounds.y = y;
                            child_box.bounds.width = word_width;
                            child_box.bounds.height = font_size;
                            current_x += word_width;
                            if font_size > line_max_ascent {
                                line_max_ascent = font_size;
                            }
                        }
                        x = current_x;
                    }
                }
            }
            child_y = y + line_max_ascent;
        }
        BoxContent::Text {
            text, font_size, ..
        } => {
            let line_height = box_node.style.line_height * *font_size;
            let words: Vec<&str> = text.split(' ').collect();
            let space_width = measure_char(' ', *font_size) * 0.3;
            let mut x = child_x;
            let mut y = child_y;
            for (i, word) in words.iter().enumerate() {
                let word_width = measure_text(word, *font_size);
                if i > 0 {
                    if x + space_width + word_width > child_x + child_width && x > child_x {
                        y += line_height;
                        x = child_x;
                    } else {
                        x += space_width;
                    }
                }
                x += word_width;
            }
            child_y = y + line_height;
        }
        BoxContent::Empty => {}
    }

    // Resolve height.
    let content_height = match box_node.style.height {
        Some(Length::Px(h)) => h,
        Some(Length::Percent(_)) => child_y - (box_node.bounds.y + border.top + padding.top),
        Some(Length::Em(em)) => em * box_node.style.font_size,
        Some(Length::Vh(v)) => v * 800.0 / 100.0, // assume 800px viewport height
        Some(Length::Vw(_)) | Some(Length::Auto) | None => {
            (child_y - (box_node.bounds.y + border.top + padding.top)).max(0.0)
        }
    };
    box_node.bounds.height =
        content_height + padding.top + padding.bottom + border.top + border.bottom;

    let _ = outer_width;
    let _ = ctx;
}

/// Public wrapper for border_widths — used by table.rs.
pub fn border_widths_pub(b: &BorderEdges) -> EdgeSizes {
    border_widths(b)
}

fn border_widths(b: &BorderEdges) -> EdgeSizes {
    EdgeSizes {
        left: if b.left.style != crate::style::BorderStyle::None {
            b.left.width
        } else {
            0.0
        },
        right: if b.right.style != crate::style::BorderStyle::None {
            b.right.width
        } else {
            0.0
        },
        top: if b.top.style != crate::style::BorderStyle::None {
            b.top.width
        } else {
            0.0
        },
        bottom: if b.bottom.style != crate::style::BorderStyle::None {
            b.bottom.width
        } else {
            0.0
        },
    }
}

// ===================== Text measurement =====================

/// Measure the width of a single character in pixels. Uses a fixed-width
/// approximation: each glyph is `font_size * 0.6` wide for monospace, and
/// we use a small per-char table for proportional fonts. This is fast and
/// close-enough for the layout engine.
pub fn measure_char(c: char, font_size: f32) -> f32 {
    let avg = match c {
        ' ' => 0.27,
        'i' | 'l' | 'I' | '|' | '!' | '.' | ',' | ';' | ':' | '\'' | '"' => 0.27,
        'm' | 'M' | 'W' | 'w' | 'O' | 'Q' => 0.78,
        _ => 0.55,
    };
    font_size * avg
}

pub fn measure_text(s: &str, font_size: f32) -> f32 {
    s.chars().map(|c| measure_char(c, font_size)).sum()
}

// ===================== Flexbox layout =====================

/// Layout a flex container's children.
#[allow(clippy::too_many_arguments)]
fn layout_flex_impl(
    children: &mut [LayoutBox],
    origin_x: f32,
    origin_y: f32,
    available_width: f32,
    viewport_width: f32,
    flex_direction: crate::style::FlexDirection,
    justify_content: crate::style::JustifyContent,
    align_items: crate::style::AlignItems,
    gap: f32,
    container_height: Option<Length>,
    container_padding_top: f32,
    container_padding_bottom: f32,
    container_font_size: f32,
) {
    use crate::style::{AlignItems, FlexDirection, JustifyContent};
    let is_row = matches!(
        flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    let n = children.len();
    if n == 0 {
        return;
    }

    let mut bases = vec![0.0f32; n];
    let grows: Vec<f32> = children.iter().map(|c| c.style.flex_grow).collect();

    for (i, child) in children.iter().enumerate() {
        if is_row {
            bases[i] = match &child.style.flex_basis {
                Some(Length::Px(v)) => *v,
                Some(Length::Percent(p)) => available_width * *p / 100.0,
                _ => match child.style.width {
                    Some(Length::Px(w)) => w,
                    Some(Length::Percent(p)) => available_width * p / 100.0,
                    _ => estimate_content_size(child, true),
                },
            };
        } else {
            bases[i] = match &child.style.flex_basis {
                Some(Length::Px(v)) => *v,
                _ => match child.style.height {
                    Some(Length::Px(h)) => h,
                    _ => 0.0,
                },
            };
        }
    }

    let total_gap = gap * (n as f32 - 1.0).max(0.0);
    let total_base: f32 = bases.iter().sum();
    let free_space = if is_row {
        available_width - total_base - total_gap
    } else {
        0.0
    };
    let total_grow: f32 = grows.iter().sum();
    let mut final_sizes = bases.clone();
    if is_row && free_space > 0.0 && total_grow > 0.0 {
        for i in 0..n {
            final_sizes[i] += free_space * (grows[i] / total_grow);
        }
    }

    let mut positions = vec![0.0f32; n];
    let mut current = 0.0f32;
    match justify_content {
        JustifyContent::FlexStart => {
            for i in 0..n {
                positions[i] = current;
                current += final_sizes[i] + gap;
            }
        }
        JustifyContent::FlexEnd => {
            let total: f32 = final_sizes.iter().sum::<f32>() + total_gap;
            current = if is_row { available_width - total } else { 0.0 };
            for i in 0..n {
                positions[i] = current;
                current += final_sizes[i] + gap;
            }
        }
        JustifyContent::Center => {
            let total: f32 = final_sizes.iter().sum::<f32>() + total_gap;
            current = if is_row {
                (available_width - total) / 2.0
            } else {
                0.0
            };
            for i in 0..n {
                positions[i] = current;
                current += final_sizes[i] + gap;
            }
        }
        JustifyContent::SpaceBetween => {
            let total: f32 = final_sizes.iter().sum::<f32>();
            let extra = if n > 1 && is_row {
                (available_width - total) / (n as f32 - 1.0)
            } else {
                gap
            };
            for i in 0..n {
                positions[i] = current;
                current += final_sizes[i] + extra;
            }
        }
        JustifyContent::SpaceAround | JustifyContent::SpaceEvenly => {
            let total: f32 = final_sizes.iter().sum::<f32>();
            let space = if is_row {
                (available_width - total) / n as f32
            } else {
                0.0
            };
            current = space / 2.0;
            for i in 0..n {
                positions[i] = current;
                current += final_sizes[i] + space;
            }
        }
    }

    let container_inner_height = match container_height {
        Some(Length::Px(h)) => h - container_padding_top - container_padding_bottom,
        _ => 0.0,
    };

    for (i, child) in children.iter_mut().enumerate() {
        let mut child_ctx = LayoutContext {
            viewport_width,
            x: origin_x,
            y: origin_y,
            available_width: if is_row {
                final_sizes[i]
            } else {
                available_width
            },
            containing_font_size: container_font_size,
        };
        if is_row {
            layout_block(child, &mut child_ctx);
            child.bounds.width = final_sizes[i];
            let cross_size = container_inner_height.max(child.bounds.height);
            match align_items {
                AlignItems::FlexStart | AlignItems::Stretch | AlignItems::Baseline => {
                    child.bounds.y = origin_y;
                    if align_items == AlignItems::Stretch && container_inner_height > 0.0 {
                        child.bounds.height = container_inner_height;
                    }
                }
                AlignItems::Center => {
                    child.bounds.y = origin_y + (cross_size - child.bounds.height) / 2.0;
                }
                AlignItems::FlexEnd => {
                    child.bounds.y = origin_y + cross_size - child.bounds.height;
                }
            }
            child.bounds.x = origin_x + positions[i];
        } else {
            child_ctx.available_width = available_width;
            layout_block(child, &mut child_ctx);
            if final_sizes[i] > 0.0 {
                child.bounds.height = final_sizes[i];
            }
            child.bounds.y = origin_y + positions[i];
            match align_items {
                AlignItems::Center => {
                    child.bounds.x = origin_x + (available_width - child.bounds.width) / 2.0;
                }
                AlignItems::FlexEnd => {
                    child.bounds.x = origin_x + available_width - child.bounds.width;
                }
                _ => {
                    child.bounds.x = origin_x;
                    if align_items == AlignItems::Stretch {
                        child.bounds.width = available_width;
                    }
                }
            }
        }
    }
}

/// Estimate the content size of a box (for auto-sized flex items).
fn estimate_content_size(box_node: &LayoutBox, is_main_axis_horizontal: bool) -> f32 {
    match &box_node.content {
        BoxContent::Text {
            text, font_size, ..
        } => {
            if is_main_axis_horizontal {
                measure_text(text, *font_size)
            } else {
                *font_size * box_node.style.line_height
            }
        }
        BoxContent::Inline(items) => {
            if is_main_axis_horizontal {
                let mut total = 0.0;
                for item in items {
                    if let InlineItem::Text(child) = item {
                        if let BoxContent::Text {
                            text, font_size, ..
                        } = &child.content
                        {
                            total += measure_text(text, *font_size);
                        }
                    }
                }
                total
            } else {
                box_node.style.font_size * box_node.style.line_height
            }
        }
        _ => 0.0,
    }
}

// ===================== Paint command emission =====================

/// Walk the layout tree and emit paint commands in z-order (background, border, text).
pub fn collect_paint_commands(root: &LayoutBox, out: &mut Vec<PaintCommand>) {
    paint_box(root, out);
}

/// Public wrapper for paint_box — used by table.rs.
pub fn paint_box_pub(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    paint_box(box_node, out);
}

fn paint_box(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let style = &box_node.style;

    // Skip invisible elements.
    if style.opacity <= 0.0 {
        return;
    }

    // Special rendering for form controls.
    let is_form_control = matches!(
        box_node.tag.as_deref(),
        Some("input") | Some("button") | Some("textarea") | Some("select")
    );

    if is_form_control {
        paint_form_control(box_node, out);
        return;
    }

    // Special rendering for <img> elements — load and draw the image.
    if box_node.tag.as_deref() == Some("img") {
        paint_image(box_node, out);
        return;
    }

    // Special rendering for <audio> elements — draw player UI.
    if box_node.tag.as_deref() == Some("audio") {
        let player_cmds = crate::media::render_audio_player(&box_node.bounds, &box_node.attrs);
        out.extend(player_cmds);
        return;
    }

    // Special rendering for <video> elements — draw player UI with poster.
    if box_node.tag.as_deref() == Some("video") {
        let base_url = crate::image::get_base_url();
        let player_cmds =
            crate::media::render_video_player(&box_node.bounds, &box_node.attrs, &base_url);
        out.extend(player_cmds);
        return;
    }

    // Special rendering for inline <svg> elements.
    if box_node.tag.as_deref() == Some("svg") {
        let child_count = match &box_node.content {
            BoxContent::Block(v)
            | BoxContent::Flex(v)
            | BoxContent::Grid(v)
            | BoxContent::Table(v) => v.len(),
            BoxContent::Inline(v) => v.len(),
            _ => 0,
        };
        paint_inline_svg(box_node, out);
        return;
    }

    // 1. Box shadows (drawn before background, behind the element).
    for shadow in &style.box_shadow {
        if !shadow.inset {
            out.push(PaintCommand::Shadow {
                x: b.x + shadow.offset_x - shadow.spread,
                y: b.y + shadow.offset_y - shadow.spread,
                w: b.width + shadow.spread * 2.0,
                h: b.height + shadow.spread * 2.0,
                blur: shadow.blur,
                color: shadow.color,
                opacity: style.opacity,
            });
        }
    }

    // 2. Background (color or gradient).
    if style.background_color.a > 0 || style.background_image.is_some() {
        if let Some(crate::style::BackgroundImage::LinearGradient { angle, stops }) =
            &style.background_image
        {
            out.push(PaintCommand::Gradient {
                x: b.x,
                y: b.y,
                w: b.width,
                h: b.height,
                angle: *angle,
                stops: stops.clone(),
                opacity: style.opacity,
            });
        } else if style.background_color.a > 0 {
            let mut color = style.background_color;
            if style.opacity < 1.0 {
                color.a = ((color.a as f32) * style.opacity) as u8;
            }
            out.push(PaintCommand::RoundedRect {
                x: b.x,
                y: b.y,
                w: b.width,
                h: b.height,
                color,
                radius: style.border_radius,
            });
        }
    }

    // 3. Borders.
    let bw = border_widths(&style.border);
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

    // 4. Content.
    match &box_node.content {
        BoxContent::Block(children)
        | BoxContent::Flex(children)
        | BoxContent::Grid(children)
        | BoxContent::Table(children) => {
            for c in children {
                paint_box(c, out);
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    paint_box(child, out);
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
        } => {
            if !text.is_empty() {
                let mut text_color = *color;
                if style.opacity < 1.0 {
                    text_color.a = ((text_color.a as f32) * style.opacity) as u8;
                }
                out.push(PaintCommand::Text {
                    x: b.x,
                    y: b.y,
                    text: text.clone(),
                    color: text_color,
                    font_size: *font_size,
                    font_weight: *font_weight,
                    italic: *font_style == crate::style::FontStyle::Italic,
                    family: font_family.clone(),
                });
            }
        }
        BoxContent::Empty => {}
    }
}

/// Paint a form control (input/button/textarea) with default browser styling.
fn paint_form_control(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let tag = box_node.tag.as_deref().unwrap_or("");
    let input_type = box_node
        .attrs
        .get("type")
        .map(|s| s.as_str())
        .unwrap_or("text");

    // Use the box's own style (border, background) but provide defaults if unset.
    let style = &box_node.style;
    let bg_color = if style.background_color.a > 0 {
        style.background_color
    } else if tag == "button"
        || (tag == "input" && (input_type == "submit" || input_type == "button"))
    {
        // Default button background: light grey.
        crate::css::Color::rgb(240, 240, 240)
    } else {
        // Default input background: white.
        crate::css::Color::rgb(255, 255, 255)
    };

    let border_color = if style.border.top.color != crate::css::Color::TRANSPARENT
        && style.border.top.width > 0.0
    {
        style.border.top.color
    } else {
        crate::css::Color::rgb(170, 170, 170)
    };
    let border_w = if style.border.top.width > 0.0 {
        style.border.top.width
    } else {
        1.0
    };

    // 1. Background.
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: b.width,
        h: b.height,
        color: bg_color,
        radius: style.border_radius,
    });

    // 2. Border (1px on all sides if not already styled).
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

    // 3. Content — for input/button, show the value attribute as text.
    if tag == "input" {
        if input_type == "submit" || input_type == "button" {
            // Button text from value attribute (default "Submit").
            let value = box_node
                .attrs
                .get("value")
                .map(|s| s.as_str())
                .unwrap_or("Submit");
            out.push(PaintCommand::Text {
                x: b.x + 12.0,
                y: b.y + (b.height - style.font_size) / 2.0,
                text: value.to_string(),
                color: style.color,
                font_size: style.font_size,
                font_weight: style.font_weight,
                italic: false,
                family: style.font_family.clone(),
            });
        } else if input_type == "text"
            || input_type == "password"
            || input_type == "email"
            || input_type == "search"
        {
            // Show the value attribute (this will be replaced by user input in interactive mode).
            let value = box_node
                .attrs
                .get("value")
                .map(|s| s.as_str())
                .unwrap_or("");
            let display_value = if input_type == "password" {
                "*".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            if !display_value.is_empty() {
                out.push(PaintCommand::Text {
                    x: b.x + 8.0,
                    y: b.y + (b.height - style.font_size) / 2.0,
                    text: display_value,
                    color: style.color,
                    font_size: style.font_size,
                    font_weight: style.font_weight,
                    italic: false,
                    family: style.font_family.clone(),
                });
            }
            // Placeholder text (greyed out) if value is empty.
            if value.is_empty() {
                if let Some(placeholder) = box_node.attrs.get("placeholder") {
                    out.push(PaintCommand::Text {
                        x: b.x + 8.0,
                        y: b.y + (b.height - style.font_size) / 2.0,
                        text: placeholder.clone(),
                        color: crate::css::Color::rgb(150, 150, 150),
                        font_size: style.font_size,
                        font_weight: style.font_weight,
                        italic: false,
                        family: style.font_family.clone(),
                    });
                }
            }
        } else if input_type == "checkbox" {
            // Draw a small checkbox.
            let size = 14.0;
            let cx = b.x + (b.width - size) / 2.0;
            let cy = b.y + (b.height - size) / 2.0;
            out.push(PaintCommand::RoundedRect {
                x: cx,
                y: cy,
                w: size,
                h: size,
                color: crate::css::Color::rgb(255, 255, 255),
                radius: Default::default(),
            });
            out.push(PaintCommand::RoundedRect {
                x: cx,
                y: cy,
                w: size,
                h: 1.0,
                color: crate::css::Color::rgb(100, 100, 100),
                radius: Default::default(),
            });
            out.push(PaintCommand::RoundedRect {
                x: cx,
                y: cy + size - 1.0,
                w: size,
                h: 1.0,
                color: crate::css::Color::rgb(100, 100, 100),
                radius: Default::default(),
            });
            out.push(PaintCommand::RoundedRect {
                x: cx,
                y: cy,
                w: 1.0,
                h: size,
                color: crate::css::Color::rgb(100, 100, 100),
                radius: Default::default(),
            });
            out.push(PaintCommand::RoundedRect {
                x: cx + size - 1.0,
                y: cy,
                w: 1.0,
                h: size,
                color: crate::css::Color::rgb(100, 100, 100),
                radius: Default::default(),
            });
            if box_node.attrs.contains_key("checked") {
                out.push(PaintCommand::RoundedRect {
                    x: cx + 3.0,
                    y: cy + 3.0,
                    w: size - 6.0,
                    h: size - 6.0,
                    color: crate::css::Color::rgb(50, 120, 220),
                    radius: Default::default(),
                });
            }
        }
    } else if tag == "button" {
        // Button text from children.
        let mut text = String::new();
        collect_text(box_node, &mut text);
        if !text.is_empty() {
            out.push(PaintCommand::Text {
                x: b.x + 12.0,
                y: b.y + (b.height - style.font_size) / 2.0,
                text,
                color: style.color,
                font_size: style.font_size,
                font_weight: style.font_weight,
                italic: false,
                family: style.font_family.clone(),
            });
        }
    } else if tag == "textarea" {
        let value = box_node
            .attrs
            .get("value")
            .map(|s| s.as_str())
            .unwrap_or("");
        if !value.is_empty() {
            out.push(PaintCommand::Text {
                x: b.x + 8.0,
                y: b.y + 8.0,
                text: value.to_string(),
                color: style.color,
                font_size: style.font_size,
                font_weight: style.font_weight,
                italic: false,
                family: style.font_family.clone(),
            });
        }
    }
}

/// Recursively collect text content from a layout box (for button labels).
fn collect_text(box_node: &LayoutBox, out: &mut String) {
    match &box_node.content {
        BoxContent::Text { text, .. } => out.push_str(text),
        BoxContent::Block(children) | BoxContent::Flex(children) => {
            for c in children {
                collect_text(c, out);
            }
        }
        BoxContent::Inline(items) => {
            for item in items {
                if let InlineItem::Text(child) = item {
                    collect_text(child, out);
                }
            }
        }
        _ => {}
    }
}

/// Paint an <img> element — load the image from src and draw it at the
/// element's bounds. Falls back to alt text if the image cannot be loaded.
fn paint_image(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let style = &box_node.style;

    // Get the src attribute.
    let src = box_node.attrs.get("src").map(|s| s.as_str()).unwrap_or("");

    // Determine display dimensions.
    let display_w = match box_node
        .attrs
        .get("width")
        .and_then(|w| w.parse::<f32>().ok())
    {
        Some(w) => w,
        None => match style.width {
            Some(Length::Px(w)) => w,
            _ => b.width.max(1.0),
        },
    };
    let display_h = match box_node
        .attrs
        .get("height")
        .and_then(|h| h.parse::<f32>().ok())
    {
        Some(h) => h,
        None => match style.height {
            Some(Length::Px(h)) => h,
            _ => b.height.max(1.0),
        },
    };

    if src.is_empty() {
        // No src — draw alt text or placeholder.
        draw_alt_text(box_node, out);
        return;
    }

    // Check if it's an SVG file or data URL.
    let is_svg = src.ends_with(".svg")
        || src.starts_with("data:image/svg+xml")
        || src.starts_with("data:image/svg");

    if is_svg {
        // Load SVG XML and render it.
        let svg_xml = if src.starts_with("data:image/svg+xml;base64,") {
            // Decode base64.
            use base64::{engine::general_purpose, Engine as _};
            let data = src.strip_prefix("data:image/svg+xml;base64,").unwrap_or("");
            match general_purpose::STANDARD.decode(data) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(_) => String::new(),
            }
        } else if src.starts_with("data:image/svg+xml,") {
            // URL-encoded SVG.
            src.strip_prefix("data:image/svg+xml,")
                .unwrap_or("")
                .to_string()
        } else {
            // Fetch from URL or read file.
            let base_url = crate::image::get_base_url();
            let resolved = crate::image::resolve_url_pub(src, &base_url);
            if resolved.starts_with("http") {
                crate::net::fetch(&resolved).unwrap_or_default()
            } else {
                std::fs::read_to_string(&resolved).unwrap_or_default()
            }
        };

        if !svg_xml.is_empty() {
            if let Some(doc) = crate::svg::parse_svg(&svg_xml) {
                let canvas_w = display_w.max(1.0) as u32;
                let canvas_h = display_h.max(1.0) as u32;
                let mut svg_canvas = crate::paint::Canvas::new_transparent(canvas_w, canvas_h);
                match crate::paint::FontRasterizer::new() {
                    Ok(rasterizer) => {
                        crate::svg::renderer::render_svg_with_rasterizer(
                            &mut svg_canvas,
                            &doc,
                            0.0,
                            0.0,
                            display_w,
                            display_h,
                            Some(&rasterizer),
                        );
                    }
                    Err(_) => {
                        crate::svg::renderer::render_svg(
                            &mut svg_canvas,
                            &doc,
                            0.0,
                            0.0,
                            display_w,
                            display_h,
                        );
                    }
                }
                out.push(PaintCommand::Image {
                    x: b.x,
                    y: b.y,
                    w: display_w,
                    h: display_h,
                    image: crate::image::Image {
                        width: canvas_w,
                        height: canvas_h,
                        pixels: svg_canvas.pixels,
                    },
                });
                return;
            }
        }
        // SVG failed — fall through to alt text.
        draw_alt_text(box_node, out);
        return;
    }

    // Try to load the image. Use the global base URL set by the browser.
    let base_url = crate::image::get_base_url();
    match crate::image::load_image(src, &base_url) {
        Some(img) => {
            out.push(PaintCommand::Image {
                x: b.x,
                y: b.y,
                w: display_w,
                h: display_h,
                image: img,
            });
        }
        None => {
            // Image failed to load — draw alt text.
            draw_alt_text(box_node, out);
        }
    }
}

/// Draw alt text for a broken image (grey box + alt text).
fn draw_alt_text(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;
    let style = &box_node.style;
    let _ = style;

    // Draw a light grey background box.
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: b.width,
        h: b.height,
        color: crate::css::Color::rgb(238, 238, 238),
        radius: Default::default(),
    });

    // Draw a 1px border.
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: b.width,
        h: 1.0,
        color: crate::css::Color::rgb(200, 200, 200),
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y + b.height - 1.0,
        w: b.width,
        h: 1.0,
        color: crate::css::Color::rgb(200, 200, 200),
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x,
        y: b.y,
        w: 1.0,
        h: b.height,
        color: crate::css::Color::rgb(200, 200, 200),
        radius: Default::default(),
    });
    out.push(PaintCommand::RoundedRect {
        x: b.x + b.width - 1.0,
        y: b.y,
        w: 1.0,
        h: b.height,
        color: crate::css::Color::rgb(200, 200, 200),
        radius: Default::default(),
    });

    // Draw alt text if present.
    if let Some(alt) = box_node.attrs.get("alt") {
        if !alt.is_empty() {
            out.push(PaintCommand::Text {
                x: b.x + 4.0,
                y: b.y + 4.0,
                text: alt.clone(),
                color: crate::css::Color::rgb(150, 150, 150),
                font_size: 12.0,
                font_weight: 400,
                italic: false,
                family: "sans-serif".to_string(),
            });
        }
    }
}

/// Paint an inline <svg> element — parse its children and render shapes.
fn paint_inline_svg(box_node: &LayoutBox, out: &mut Vec<PaintCommand>) {
    let b = box_node.bounds;

    // Try to get the original SVG XML from:
    // 1. The global STANDALONE_SVG (set by main.rs for standalone SVG files)
    // 2. The data-falco-original attribute (for inline SVG with original content)
    // 3. Fallback: reconstruct from layout tree (lossy)
    let svg_xml = if box_node.attrs.get("data-falco-standalone").is_some() {
        // Use the global standalone SVG content.
        if let Some(svg) = crate::get_standalone_svg() {
            svg.to_string()
        } else {
            // Fallback if global is not set.
            format!(
                "<svg width=\"{}\" height=\"{}\"></svg>",
                b.width as u32, b.height as u32
            )
        }
    } else if let Some(original) = box_node.attrs.get("data-falco-original") {
        original.replace("&amp;", "&").replace("&quot;", "\"")
    } else {
        // Fallback: reconstruct from layout tree.
        let viewbox = box_node
            .attrs
            .get("viewBox")
            .or_else(|| box_node.attrs.get("viewbox"));
        let mut xml = if let Some(vb) = viewbox {
            format!(
                "<svg width=\"{}\" height=\"{}\" viewBox=\"{}\">",
                b.width as u32, b.height as u32, vb
            )
        } else {
            format!(
                "<svg width=\"{}\" height=\"{}\">",
                b.width as u32, b.height as u32
            )
        };
        serialize_svg_children(box_node, &mut xml);
        xml.push_str("</svg>");
        xml
    };

    // Parse and render the SVG onto a temporary canvas, then emit as Image.
    if let Some(doc) = crate::svg::parse_svg(&svg_xml) {
        let canvas_w = b.width.max(1.0) as u32;
        let canvas_h = b.height.max(1.0) as u32;
        let mut svg_canvas = crate::paint::Canvas::new_transparent(canvas_w, canvas_h);
        // Pass the font rasterizer so SVG <text> elements render with real glyphs.
        match crate::paint::FontRasterizer::new() {
            Ok(rasterizer) => {
                crate::svg::renderer::render_svg_with_rasterizer(
                    &mut svg_canvas,
                    &doc,
                    0.0,
                    0.0,
                    b.width,
                    b.height,
                    Some(&rasterizer),
                );
            }
            Err(_) => {
                crate::svg::renderer::render_svg(
                    &mut svg_canvas,
                    &doc,
                    0.0,
                    0.0,
                    b.width,
                    b.height,
                );
            }
        }
        out.push(PaintCommand::Image {
            x: b.x,
            y: b.y,
            w: b.width,
            h: b.height,
            image: crate::image::Image {
                width: canvas_w,
                height: canvas_h,
                pixels: svg_canvas.pixels,
            },
        });
    }
}

/// Serialize layout box children back to SVG XML (for inline SVG rendering).
fn serialize_svg_children(box_node: &LayoutBox, out: &mut String) {
    match &box_node.content {
        BoxContent::Block(children)
        | BoxContent::Flex(children)
        | BoxContent::Grid(children)
        | BoxContent::Table(children) => {
            for c in children {
                serialize_one_svg_child(c, out);
            }
        }
        BoxContent::Inline(items) => {
            // SVG children might be in Inline layout — extract LayoutBoxes.
            for item in items {
                if let InlineItem::Text(b) = item {
                    serialize_one_svg_child(b, out);
                }
            }
        }
        _ => {}
    }
}

fn serialize_one_svg_child(c: &LayoutBox, out: &mut String) {
    if let Some(tag) = &c.tag {
        match tag.as_str() {
            "rect" => {
                // Use original SVG attributes, not layout bounds.
                let x = c
                    .attrs
                    .get("x")
                    .cloned()
                    .unwrap_or_else(|| c.bounds.x.to_string());
                let y = c
                    .attrs
                    .get("y")
                    .cloned()
                    .unwrap_or_else(|| c.bounds.y.to_string());
                let w = c
                    .attrs
                    .get("width")
                    .cloned()
                    .unwrap_or_else(|| c.bounds.width.to_string());
                let h = c
                    .attrs
                    .get("height")
                    .cloned()
                    .unwrap_or_else(|| c.bounds.height.to_string());
                let fill = c.attrs.get("fill").cloned().unwrap_or_else(|| {
                    if c.style.background_color.a > 0 {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            c.style.background_color.r,
                            c.style.background_color.g,
                            c.style.background_color.b
                        )
                    } else {
                        "none".to_string()
                    }
                });
                let stroke = c
                    .attrs
                    .get("stroke")
                    .map(|s| format!(" stroke=\"{}\"", s))
                    .unwrap_or_default();
                let sw = c
                    .attrs
                    .get("stroke-width")
                    .map(|s| format!(" stroke-width=\"{}\"", s))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}{}/>",
                    x, y, w, h, fill, stroke, sw
                ));
            }
            "circle" => {
                let cx = c
                    .attrs
                    .get("cx")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.x + c.bounds.width / 2.0).to_string());
                let cy = c
                    .attrs
                    .get("cy")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.y + c.bounds.height / 2.0).to_string());
                let r = c
                    .attrs
                    .get("r")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.width.min(c.bounds.height) / 2.0).to_string());
                let fill = c.attrs.get("fill").cloned().unwrap_or_else(|| {
                    if c.style.background_color.a > 0 {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            c.style.background_color.r,
                            c.style.background_color.g,
                            c.style.background_color.b
                        )
                    } else {
                        "none".to_string()
                    }
                });
                out.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
                    cx, cy, r, fill
                ));
            }
            "ellipse" => {
                let cx = c
                    .attrs
                    .get("cx")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.x + c.bounds.width / 2.0).to_string());
                let cy = c
                    .attrs
                    .get("cy")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.y + c.bounds.height / 2.0).to_string());
                let rx = c
                    .attrs
                    .get("rx")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.width / 2.0).to_string());
                let ry = c
                    .attrs
                    .get("ry")
                    .cloned()
                    .unwrap_or_else(|| (c.bounds.height / 2.0).to_string());
                let fill = c.attrs.get("fill").cloned().unwrap_or("none".to_string());
                out.push_str(&format!(
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"/>",
                    cx, cy, rx, ry, fill
                ));
            }
            "line" => {
                let x1 = c.attrs.get("x1").cloned().unwrap_or_default();
                let y1 = c.attrs.get("y1").cloned().unwrap_or_default();
                let x2 = c.attrs.get("x2").cloned().unwrap_or_default();
                let y2 = c.attrs.get("y2").cloned().unwrap_or_default();
                let stroke = c
                    .attrs
                    .get("stroke")
                    .cloned()
                    .unwrap_or_else(|| "black".to_string());
                let sw = c
                    .attrs
                    .get("stroke-width")
                    .cloned()
                    .unwrap_or_else(|| "1".to_string());
                out.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>", x1, y1, x2, y2, stroke, sw));
            }
            "path" => {
                let d = c.attrs.get("d").cloned().unwrap_or_default();
                let fill = c
                    .attrs
                    .get("fill")
                    .cloned()
                    .unwrap_or_else(|| "none".to_string());
                let stroke = c
                    .attrs
                    .get("stroke")
                    .map(|s| format!(" stroke=\"{}\"", s))
                    .unwrap_or_default();
                let sw = c
                    .attrs
                    .get("stroke-width")
                    .map(|s| format!(" stroke-width=\"{}\"", s))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<path d=\"{}\" fill=\"{}\"{}{}/>",
                    d, fill, stroke, sw
                ));
            }
            "polygon" | "polyline" => {
                let points = c.attrs.get("points").cloned().unwrap_or_default();
                let fill = c
                    .attrs
                    .get("fill")
                    .cloned()
                    .unwrap_or_else(|| "none".to_string());
                let stroke = c
                    .attrs
                    .get("stroke")
                    .map(|s| format!(" stroke=\"{}\"", s))
                    .unwrap_or_default();
                let sw = c
                    .attrs
                    .get("stroke-width")
                    .map(|s| format!(" stroke-width=\"{}\"", s))
                    .unwrap_or_default();
                let tag_name = tag.as_str();
                out.push_str(&format!(
                    "<{} points=\"{}\" fill=\"{}\"{}{}/>",
                    tag_name, points, fill, stroke, sw
                ));
            }
            "text" => {
                // For text, serialize with attributes.
                let mut attrs_str = String::new();
                for (k, v) in &c.attrs {
                    if k != "style" && k != "class" {
                        attrs_str.push_str(&format!(" {}=\"{}\"", k, v));
                    }
                }
                out.push_str(&format!("<text{}>", attrs_str));
                serialize_svg_children(c, out);
                out.push_str("</text>");
            }
            "g" => {
                let mut attrs_str = String::new();
                for (k, v) in &c.attrs {
                    if k != "style" && k != "class" {
                        attrs_str.push_str(&format!(" {}=\"{}\"", k, v));
                    }
                }
                out.push_str(&format!("<g{}>", attrs_str));
                serialize_svg_children(c, out);
                out.push_str("</g>");
            }
            _ => serialize_svg_children(c, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_text() {
        let w = measure_text("hello", 16.0);
        assert!(w > 0.0);
    }
}
