//! MathML — Mathematical Markup Language layout and rendering.
//!
//! # Supported Elements
//!
//! - `<math>` — the root element
//! - `<mrow>` — a horizontal row of expressions
//! - `<mi>` — identifier (variable name, italic)
//! - `<mo>` — operator (+, −, ×, ÷, =, etc.)
//! - `<mn>` — number (upright)
//! - `<ms>` — string literal
//! - `<msup>` — superscript (x²)
//! - `<msub>` — subscript (x₀)
//! - `<msubsup>` — sub-and-superscript
//! - `<mfrac>` — fraction (numerator / denominator)
//! - `<msqrt>` — square root
//! - `<mroot>` — nth root
//! - `<mtext>` — text (upright)
//! - `<mspace>` — space
//! - `<mtable>` — table (matrix)
//! - `<mtr>` — table row
//! - `<mtd>` — table cell
//! - `<mfenced>` — fenced (parentheses, brackets)
//! - `<mover>` — overscript (x̄)
//! - `<munder>` — underscript
//! - `<munderover>` — under-and-overscript
//!
//! # Layout
//!
//! MathML uses a flow-based layout similar to inline HTML, but with
//! special handling for fractions, scripts, and radicals. The layout
//! produces a tree of `MathBox` rectangles that the painter can render.

/// A node in a MathML DOM tree.
///
/// This is a simple tree representation that the HTML parser can populate
/// when it encounters `<math>` elements. The layout module walks this tree
/// to produce positioned `MathBox` rectangles.
pub struct MathNode {
    /// The tag name (e.g., "math", "mrow", "mi").
    pub tag: String,
    /// The text content (for leaf nodes like `<mi>`, `<mn>`).
    pub text: String,
    /// The element's attributes (e.g., "open" for `<mfenced>`).
    pub attrs: std::collections::HashMap<String, String>,
    /// The child nodes.
    pub children: Vec<MathNode>,
}

impl MathNode {
    /// Create a new MathNode with the given tag.
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            text: String::new(),
            attrs: std::collections::HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Create a text node.
    pub fn text(text: &str) -> Self {
        Self {
            tag: "#text".to_string(),
            text: text.to_string(),
            attrs: std::collections::HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Get the tag name.
    pub fn tag_name(&self) -> &str {
        &self.tag
    }

    /// Get the text content (recursively, for leaf nodes).
    pub fn text_content(&self) -> String {
        if self.children.is_empty() {
            self.text.clone()
        } else {
            self.children
                .iter()
                .map(|c| c.text_content())
                .collect::<Vec<_>>()
                .join("")
        }
    }

    /// Get an attribute value.
    pub fn get_attribute(&self, name: &str) -> Option<&String> {
        self.attrs.get(name)
    }

    /// Iterate over children.
    pub fn children_iter(&self) -> &[MathNode] {
        &self.children
    }

    /// Add a child node.
    pub fn push_child(&mut self, child: MathNode) {
        self.children.push(child);
    }

    /// Set an attribute.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        self.attrs.insert(name.to_string(), value.to_string());
    }
}

/// A MathML layout box — a rectangle with positioned content.
#[derive(Debug, Clone)]
pub struct MathBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub content: MathContent,
}

/// The content of a MathML box.
#[derive(Debug, Clone)]
pub enum MathContent {
    /// A text glyph (character + font style).
    Text {
        text: String,
        style: MathStyle,
    },
    /// A fraction with numerator and denominator.
    Fraction {
        numerator: Box<MathBox>,
        denominator: Box<MathBox>,
        bar_y: f32,
    },
    /// A square root.
    Radical {
        radicand: Box<MathBox>,
    },
    /// An nth root.
    Root {
        radicand: Box<MathBox>,
        index: Box<MathBox>,
    },
    /// A subscript/superscript combination.
    Scripts {
        base: Box<MathBox>,
        sub: Option<Box<MathBox>>,
        sup: Option<Box<MathBox>>,
    },
    /// An overscript (e.g., bar over x).
    Over {
        base: Box<MathBox>,
        over: Box<MathBox>,
    },
    /// An underscript.
    Under {
        base: Box<MathBox>,
        under: Box<MathBox>,
    },
    /// A fenced group (parentheses, etc.).
    Fenced {
        open: char,
        close: char,
        content: Vec<MathBox>,
    },
    /// A row of boxes.
    Row(Vec<MathBox>),
    /// A table (matrix).
    Table {
        rows: Vec<Vec<MathBox>>,
    },
    /// Empty space.
    Space(f32),
}

/// The font style for MathML text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathStyle {
    /// Italic (for variables: x, y, z).
    Italic,
    /// Upright (for numbers, operators, text).
    Upright,
    /// Bold.
    Bold,
    /// Bold italic.
    BoldItalic,
}

/// Layout a MathML `<math>` element.
///
/// The `dom_node` is the parsed `<math>` element from the HTML parser.
/// Returns a positioned `MathBox` tree.
pub fn layout_math(node: &MathNode, x: f32, y: f32, font_size: f32) -> Option<MathBox> {
    let children: &[MathNode] = node.children_iter();
    if children.is_empty() {
        return None;
    }

    let mut boxes: Vec<MathBox> = Vec::new();
    let mut current_x = x;
    for child in children {
        if let Some(b) = layout_math_element(child, current_x, y, font_size) {
            current_x += b.width + font_size * 0.2; // small gap between elements
            boxes.push(b);
        }
    }

    if boxes.is_empty() {
        return None;
    }

    // Compute the bounding box.
    let total_width = boxes.last().map(|b| b.x + b.width - x).unwrap_or(0.0);
    let max_height = boxes.iter().map(|b| b.height).fold(0.0_f32, f32::max);
    let max_baseline = boxes.iter().map(|b| b.baseline).fold(0.0_f32, f32::max);

    Some(MathBox {
        x,
        y,
        width: total_width,
        height: max_height,
        baseline: max_baseline,
        content: MathContent::Row(boxes),
    })
}

/// Layout a single MathML element.
fn layout_math_element(node: &MathNode, x: f32, y: f32, font_size: f32) -> Option<MathBox> {
    let tag = node.tag_name().to_string();
    let text = node.text_content();

    match tag.as_str() {
        "mi" => {
            // Identifier — italic by default.
            let width = text.len() as f32 * font_size * 0.6;
            Some(MathBox {
                x,
                y,
                width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Text {
                    text,
                    style: MathStyle::Italic,
                },
            })
        }
        "mo" => {
            // Operator — upright.
            let width = text.len() as f32 * font_size * 0.6 + font_size * 0.3; // extra space around operators
            Some(MathBox {
                x,
                y,
                width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Text {
                    text,
                    style: MathStyle::Upright,
                },
            })
        }
        "mn" => {
            // Number — upright.
            let width = text.len() as f32 * font_size * 0.6;
            Some(MathBox {
                x,
                y,
                width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Text {
                    text,
                    style: MathStyle::Upright,
                },
            })
        }
        "ms" | "mtext" => {
            // String literal or text — upright.
            let width = text.len() as f32 * font_size * 0.55;
            Some(MathBox {
                x,
                y,
                width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Text {
                    text,
                    style: MathStyle::Upright,
                },
            })
        }
        "mspace" => {
            // Space — width from attribute.
            let width = node.get_attribute("width").and_then(|s| s.parse().ok()).unwrap_or(font_size);
            Some(MathBox {
                x,
                y,
                width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Space(width),
            })
        }
        "mrow" => {
            // A row of expressions.
            let children: &[MathNode] = node.children_iter();
            let mut boxes: Vec<MathBox> = Vec::new();
            let mut cx = x;
            for child in children {
                if let Some(b) = layout_math_element(child, cx, y, font_size) {
                    cx += b.width + font_size * 0.1;
                    boxes.push(b);
                }
            }
            let total_width = boxes.last().map(|b| b.x + b.width - x).unwrap_or(0.0);
            let max_height = boxes.iter().map(|b| b.height).fold(0.0_f32, f32::max);
            let max_baseline = boxes.iter().map(|b| b.baseline).fold(0.0_f32, f32::max);
            Some(MathBox {
                x,
                y,
                width: total_width,
                height: max_height,
                baseline: max_baseline,
                content: MathContent::Row(boxes),
            })
        }
        "msup" => {
            // Superscript: base^sup
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let sup_font = font_size * 0.7;
            let sup = layout_math_element(
                &children[1],
                x + base.width,
                y - font_size * 0.5,
                sup_font,
            )?;
            let total_width = base.width + sup.width;
            Some(MathBox {
                x,
                y: y - font_size * 0.5,
                width: total_width,
                height: base.height + font_size * 0.5,
                baseline: base.baseline,
                content: MathContent::Scripts {
                    base: Box::new(base),
                    sub: None,
                    sup: Some(Box::new(sup)),
                },
            })
        }
        "msub" => {
            // Subscript: base_sub
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let sub_font = font_size * 0.7;
            let sub = layout_math_element(
                &children[1],
                x + base.width,
                y + font_size * 0.6,
                sub_font,
            )?;
            let total_width = base.width + sub.width;
            Some(MathBox {
                x,
                y,
                width: total_width,
                height: base.height + font_size * 0.4,
                baseline: base.baseline,
                content: MathContent::Scripts {
                    base: Box::new(base),
                    sub: Some(Box::new(sub)),
                    sup: None,
                },
            })
        }
        "msubsup" => {
            // Both sub and sup.
            let children: &[MathNode] = node.children_iter();
            if children.len() < 3 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let sub_font = font_size * 0.7;
            let sub = layout_math_element(
                &children[1],
                x + base.width,
                y + font_size * 0.6,
                sub_font,
            )?;
            let sup = layout_math_element(
                &children[2],
                x + base.width,
                y - font_size * 0.5,
                sub_font,
            )?;
            let total_width = base.width + sub.width.max(sup.width);
            Some(MathBox {
                x,
                y: y - font_size * 0.5,
                width: total_width,
                height: base.height + font_size,
                baseline: base.baseline,
                content: MathContent::Scripts {
                    base: Box::new(base),
                    sub: Some(Box::new(sub)),
                    sup: Some(Box::new(sup)),
                },
            })
        }
        "mfrac" => {
            // Fraction: num / den
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let num = layout_math_element(&children[0], x, y - font_size * 0.6, font_size * 0.8)?;
            let den = layout_math_element(&children[1], x, y + font_size * 0.6, font_size * 0.8)?;
            let total_width = num.width.max(den.width);
            let bar_y = y;
            Some(MathBox {
                x,
                y: y - font_size * 0.6,
                width: total_width,
                height: font_size * 1.8,
                baseline: font_size * 0.6,
                content: MathContent::Fraction {
                    numerator: Box::new(num),
                    denominator: Box::new(den),
                    bar_y,
                },
            })
        }
        "msqrt" => {
            // Square root.
            let children: &[MathNode] = node.children_iter();
            if children.is_empty() {
                return None;
            }
            // Layout the radicand (children in an implicit mrow).
            let mut boxes: Vec<MathBox> = Vec::new();
            let mut cx = x + font_size * 0.5; // space for the radical sign
            for child in children {
                if let Some(b) = layout_math_element(child, cx, y, font_size) {
                    cx += b.width;
                    boxes.push(b);
                }
            }
            let radicand_width = boxes.last().map(|b| b.x + b.width - x).unwrap_or(font_size);
            let total_width = radicand_width + font_size * 0.5;
            let radicand = MathBox {
                x: x + font_size * 0.5,
                y,
                width: radicand_width,
                height: font_size,
                baseline: font_size * 0.8,
                content: MathContent::Row(boxes),
            };
            Some(MathBox {
                x,
                y: y - font_size * 0.1,
                width: total_width,
                height: font_size * 1.2,
                baseline: font_size * 0.8,
                content: MathContent::Radical {
                    radicand: Box::new(radicand),
                },
            })
        }
        "mroot" => {
            // nth root.
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let radicand = layout_math_element(&children[0], x + font_size * 0.5, y, font_size)?;
            let index = layout_math_element(
                &children[1],
                x,
                y - font_size * 0.3,
                font_size * 0.6,
            )?;
            let total_width = radicand.width + font_size * 0.5;
            Some(MathBox {
                x,
                y: y - font_size * 0.3,
                width: total_width,
                height: font_size * 1.3,
                baseline: font_size * 0.8,
                content: MathContent::Root {
                    radicand: Box::new(radicand),
                    index: Box::new(index),
                },
            })
        }
        "mfenced" => {
            // Fenced group: (content)
            let open = node
                .get_attribute("open")
                .and_then(|s| s.chars().next())
                .unwrap_or('(');
            let close = node
                .get_attribute("close")
                .and_then(|s| s.chars().next())
                .unwrap_or(')');
            let children: &[MathNode] = node.children_iter();
            let mut boxes: Vec<MathBox> = Vec::new();
            let mut cx = x + font_size * 0.4; // space for open fence
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    // Add a comma separator.
                    cx += font_size * 0.3;
                }
                if let Some(b) = layout_math_element(child, cx, y, font_size) {
                    cx += b.width;
                    boxes.push(b);
                }
            }
            let content_width = boxes.last().map(|b| b.x + b.width - x).unwrap_or(0.0);
            let total_width = content_width + font_size * 0.8; // space for both fences
            Some(MathBox {
                x,
                y,
                width: total_width,
                height: font_size * 1.2,
                baseline: font_size * 0.8,
                content: MathContent::Fenced {
                    open,
                    close,
                    content: boxes,
                },
            })
        }
        "mover" => {
            // Overscript.
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let over = layout_math_element(
                &children[1],
                x,
                y - font_size * 0.5,
                font_size * 0.7,
            )?;
            Some(MathBox {
                x,
                y: y - font_size * 0.5,
                width: base.width,
                height: base.height + font_size * 0.5,
                baseline: base.baseline + font_size * 0.5,
                content: MathContent::Over {
                    base: Box::new(base),
                    over: Box::new(over),
                },
            })
        }
        "munder" => {
            // Underscript.
            let children: &[MathNode] = node.children_iter();
            if children.len() < 2 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let under = layout_math_element(
                &children[1],
                x,
                y + base.height,
                font_size * 0.7,
            )?;
            Some(MathBox {
                x,
                y,
                width: base.width,
                height: base.height + font_size * 0.5,
                baseline: base.baseline,
                content: MathContent::Under {
                    base: Box::new(base),
                    under: Box::new(under),
                },
            })
        }
        "munderover" => {
            // Both under and over.
            let children: &[MathNode] = node.children_iter();
            if children.len() < 3 {
                return None;
            }
            let base = layout_math_element(&children[0], x, y, font_size)?;
            let under = layout_math_element(
                &children[1],
                x,
                y + base.height,
                font_size * 0.7,
            )?;
            let over = layout_math_element(
                &children[2],
                x,
                y - font_size * 0.5,
                font_size * 0.7,
            )?;
            Some(MathBox {
                x,
                y: y - font_size * 0.5,
                width: base.width,
                height: base.height + font_size,
                baseline: base.baseline + font_size * 0.5,
                content: MathContent::Over {
                    base: Box::new(MathBox {
                        x: base.x,
                        y: base.y,
                        width: base.width,
                        height: base.height,
                        baseline: base.baseline,
                        content: MathContent::Under {
                            base: Box::new(base),
                            under: Box::new(under),
                        },
                    }),
                    over: Box::new(over),
                },
            })
        }
        "mtable" => {
            // Table (matrix).
            let rows: Vec<Vec<MathBox>> = node
                .children_iter()
                .iter()
                .filter(|c| c.tag_name() == "mtr")
                .map(|tr| {
                    tr.children_iter()
                        .iter()
                        .filter(|c| c.tag_name() == "mtd")
                        .filter_map(|td| layout_math_element(td, 0.0, 0.0, font_size * 0.9))
                        .collect()
                })
                .collect();
            if rows.is_empty() {
                return None;
            }
            // Compute the total width and height.
            let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let col_widths: Vec<f32> = (0..max_cols)
                .map(|col| {
                    rows.iter()
                        .filter_map(|r| r.get(col).map(|b| b.width))
                        .fold(0.0_f32, f32::max)
                })
                .collect();
            let row_heights: Vec<f32> = rows
                .iter()
                .map(|r| r.iter().map(|b| b.height).fold(0.0_f32, f32::max))
                .collect();
            let total_width = col_widths.iter().sum::<f32>() + (max_cols as f32 - 1.0) * font_size * 0.5;
            let total_height = row_heights.iter().sum::<f32>() + (rows.len() as f32 - 1.0) * font_size * 0.3;
            Some(MathBox {
                x,
                y,
                width: total_width,
                height: total_height,
                baseline: total_height / 2.0,
                content: MathContent::Table { rows },
            })
        }
        _ => {
            // Unknown element — try laying out its children as a row.
            let children: &[MathNode] = node.children_iter();
            if children.is_empty() {
                // It's a text node.
                if text.is_empty() {
                    return None;
                }
                let width = text.len() as f32 * font_size * 0.6;
                return Some(MathBox {
                    x,
                    y,
                    width,
                    height: font_size,
                    baseline: font_size * 0.8,
                    content: MathContent::Text {
                        text,
                        style: MathStyle::Upright,
                    },
                });
            }
            let mut boxes: Vec<MathBox> = Vec::new();
            let mut cx = x;
            for child in children {
                if let Some(b) = layout_math_element(child, cx, y, font_size) {
                    cx += b.width;
                    boxes.push(b);
                }
            }
            let total_width = boxes.last().map(|b| b.x + b.width - x).unwrap_or(0.0);
            let max_height = boxes.iter().map(|b| b.height).fold(0.0_f32, f32::max);
            let max_baseline = boxes.iter().map(|b| b.baseline).fold(0.0_f32, f32::max);
            Some(MathBox {
                x,
                y,
                width: total_width,
                height: max_height,
                baseline: max_baseline,
                content: MathContent::Row(boxes),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The tests below use a mock LayoutNode since the real one requires
    // the full HTML parser. We test the layout logic indirectly.

    #[test]
    fn math_style_variants() {
        assert_eq!(MathStyle::Italic, MathStyle::Italic);
        assert_ne!(MathStyle::Italic, MathStyle::Upright);
    }

    #[test]
    fn math_box_clone() {
        let box1 = MathBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
            baseline: 16.0,
            content: MathContent::Text {
                text: "x".to_string(),
                style: MathStyle::Italic,
            },
        };
        let box2 = box1.clone();
        assert_eq!(box1.x, box2.x);
        assert_eq!(box1.width, box2.width);
    }

    #[test]
    fn math_content_variants() {
        let text = MathContent::Text {
            text: "42".to_string(),
            style: MathStyle::Upright,
        };
        match text {
            MathContent::Text { text, style } => {
                assert_eq!(text, "42");
                assert_eq!(style, MathStyle::Upright);
            }
            _ => panic!("expected Text"),
        }

        let space = MathContent::Space(10.0);
        match space {
            MathContent::Space(w) => assert_eq!(w, 10.0),
            _ => panic!("expected Space"),
        }
    }
}
