//! Style — cascade, inheritance, computed values.
//!
//! For each element we compute a `ComputedStyle` containing the resolved
//! values of every property the layout engine and painter care about.
//! Inheritance flows from parent to child for inheritable properties

pub mod extra;

use crate::css::{Color, Rule, Stylesheet, Unit, Value};
use crate::dom::Node;
use std::collections::HashMap;

/// All computed properties the engine cares about. We keep this small and
/// focused — adding new properties just means adding fields here and
/// handling them in the cascade + layout/paint code.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    pub display: Display,
    pub position: Position,
    pub color: Color,
    pub background_color: Color,
    pub background_image: Option<BackgroundImage>,
    pub font_size: f32, // px
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_family: String,
    pub line_height: f32, // multiplier (1.5 = 1.5em)
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: BorderEdges,
    pub border_radius: BorderRadius,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
    pub z_index: Option<i32>,
    pub overflow: Overflow,
    pub opacity: f32,
    pub box_shadow: Vec<BoxShadow>,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub flex_wrap: FlexWrap,
    pub gap: f32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Option<Length>,
    pub custom_properties: HashMap<String, String>,
    pub box_sizing: BoxSizing,
    /// Generic store for all recognized-but-not-yet-applied CSS properties.
    /// This allows Falco to accept 300+ CSS properties even if only ~80
    /// are actually wired to layout/paint. The rest are stored here for
    /// future use, inheritance, and CSS variable resolution.
    pub property_store: HashMap<String, String>,
    /// Transform property (translate, rotate, scale).
    pub transform: Option<Transform>,
    /// Object-fit for images.
    pub object_fit: ObjectFit,
    /// Float property (left, right, none).
    pub float: Float,
    /// Clear property (left, right, both, none).
    pub clear: Clear,
    /// Outline (shorthand).
    pub outline_width: f32,
    pub outline_color: Color,
    pub outline_style: BorderStyle,
    /// Vertical-align.
    pub vertical_align: VerticalAlign,
    /// Cursor type.
    pub cursor: CursorType,
    /// Text decoration (stored here too for convenience).
    pub text_decoration: TextDecoration,
    pub text_transform: TextTransform,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    /// List style.
    pub list_style_type: ListStyleType,
    /// Table layout.
    pub table_layout: TableLayout,
    pub border_collapse: BorderCollapse,
    /// Visibility.
    pub visibility: Visibility,
    /// Animation/transition (stored as raw strings for future use).
    pub animation: Option<String>,
    pub transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    Translate(f32, f32),
    Rotate(f32),
    Scale(f32, f32),
    Multiple(Vec<TransformOp>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Translate(f32, f32),
    Rotate(f32),
    Scale(f32, f32),
    Skew(f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
    Sub,
    Super,
    TextTop,
    TextBottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CursorType {
    #[default]
    Auto,
    Default,
    Pointer,
    Text,
    Wait,
    Crosshair,
    NotAllowed,
    Help,
    Move,
    Grab,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
    Overline,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
    FullWidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    None,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TableLayout {
    #[default]
    Auto,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WhiteSpace {
    Normal,
    Pre,
    PreWrap,
    Nowrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overflow {
    Visible,
    Hidden,
    Auto,
    Scroll,
}

#[derive(Debug, Clone)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

#[derive(Debug, Clone)]
pub enum BackgroundImage {
    LinearGradient {
        angle: f32,
        stops: Vec<(f32, Color)>,
    },
    Url(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: "sans-serif".into(),
            line_height: 1.5,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            margin: EdgeSizes::default(),
            padding: EdgeSizes::default(),
            border: BorderEdges::default(),
            border_radius: BorderRadius::default(),
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            top: None,
            right: None,
            bottom: None,
            left: None,
            z_index: None,
            overflow: Overflow::Visible,
            opacity: 1.0,
            box_shadow: Vec::new(),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_wrap: FlexWrap::Nowrap,
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: None,
            custom_properties: HashMap::new(),
            box_sizing: BoxSizing::ContentBox,
            property_store: HashMap::new(),
            transform: None,
            object_fit: ObjectFit::Fill,
            float: Float::None,
            clear: Clear::None,
            outline_width: 0.0,
            outline_color: Color::TRANSPARENT,
            outline_style: BorderStyle::None,
            vertical_align: VerticalAlign::Baseline,
            cursor: CursorType::Auto,
            text_decoration: TextDecoration::None,
            text_transform: TextTransform::None,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            list_style_type: ListStyleType::Disc,
            table_layout: TableLayout::Auto,
            border_collapse: BorderCollapse::Separate,
            visibility: Visibility::Visible,
            animation: None,
            transition: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Table,
    TableRow,
    TableCell,
    TableHeaderGroup,
    TableFooterGroup,
    TableRowGroup,
    TableColumnGroup,
    TableColumn,
    TableCaption,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BorderEdges {
    pub left: BorderEdge,
    pub right: BorderEdge,
    pub top: BorderEdge,
    pub bottom: BorderEdge,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BorderEdge {
    pub width: f32,
    pub color: Color,
    pub style: BorderStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
    Dotted,
    Dashed,
}

#[derive(Debug, Clone, Copy)]
pub enum Length {
    Auto,
    Px(f32),
    Em(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
}

impl ComputedStyle {
    /// Browser-default UA styles for known tags. We return an Option<ComputedStyle>
    /// that callers can layer user styles on top of.
    pub fn ua_for(tag: &str) -> ComputedStyle {
        let mut s = ComputedStyle::default();
        match tag {
            // Head elements — never render.
            "head" | "title" | "meta" | "link" | "base" | "script" | "style" | "noscript" => {
                s.display = Display::None;
            }
            // Block-level.
            "html" | "body" | "div" | "section" | "article" | "header" | "footer" | "main"
            | "nav" | "aside" | "p" | "ul" | "ol" | "li" | "blockquote" | "pre" | "form"
            | "fieldset" | "hr" | "table" | "thead" | "tbody" | "tr" | "td" | "th" | "h1"
            | "h2" | "h3" | "h4" | "h5" | "h6" | "figure" | "figcaption" | "details"
            | "summary" | "dialog" => {
                s.display = Display::Block;
            }
            // Inline.
            "a" | "span" | "b" | "strong" | "i" | "em" | "code" | "tt" | "u" | "small" | "sub"
            | "sup" | "label" | "abbr" | "cite" | "q" | "mark" | "time" | "kbd" | "samp"
            | "var" | "input" | "br" | "button" | "select" | "textarea" => {
                s.display = Display::Inline;
            }
            // Inline-block — images respect width/height but flow inline.
            "img" => {
                s.display = Display::InlineBlock;
            }
            _ => {
                s.display = Display::Inline;
            }
        }
        // Heading sizes — relative to default 16px.
        match tag {
            "h1" => {
                s.font_size = 32.0;
                s.font_weight = 700;
                s.margin.top = 21.4;
                s.margin.bottom = 21.4;
            }
            "h2" => {
                s.font_size = 24.0;
                s.font_weight = 700;
                s.margin.top = 19.1;
                s.margin.bottom = 19.1;
            }
            "h3" => {
                s.font_size = 18.7;
                s.font_weight = 700;
                s.margin.top = 18.7;
                s.margin.bottom = 18.7;
            }
            "h4" => {
                s.font_size = 16.0;
                s.font_weight = 700;
                s.margin.top = 21.3;
                s.margin.bottom = 21.3;
            }
            "h5" => {
                s.font_size = 13.3;
                s.font_weight = 700;
                s.margin.top = 22.2;
                s.margin.bottom = 22.2;
            }
            "h6" => {
                s.font_size = 10.7;
                s.font_weight = 700;
                s.margin.top = 24.6;
                s.margin.bottom = 24.6;
            }
            "p" => {
                s.margin.top = 16.0;
                s.margin.bottom = 16.0;
            }
            "ul" | "ol" => {
                s.margin.top = 16.0;
                s.margin.bottom = 16.0;
                s.padding.left = 40.0;
            }
            "blockquote" => {
                s.margin.top = 16.0;
                s.margin.bottom = 16.0;
                s.padding.left = 20.0;
                s.color = Color::rgb(100, 100, 100);
            }
            "hr" => {
                s.margin.top = 8.0;
                s.margin.bottom = 8.0;
                s.border.bottom = BorderEdge {
                    width: 1.0,
                    color: Color::rgb(200, 200, 200),
                    style: BorderStyle::Solid,
                };
            }
            "pre" => {
                s.font_family = "monospace".into();
                s.font_size = 13.0;
                s.margin.top = 16.0;
                s.margin.bottom = 16.0;
            }
            "code" => {
                s.font_family = "monospace".into();
            }
            "b" | "strong" => {
                s.font_weight = 700;
            }
            "i" | "em" => {
                s.font_style = FontStyle::Italic;
            }
            "small" => {
                s.font_size = 12.0;
            }
            // Form element defaults — give them a sensible size and padding.
            "input" => {
                s.padding.top = 6.0;
                s.padding.bottom = 6.0;
                s.padding.left = 8.0;
                s.padding.right = 8.0;
                s.width = Some(Length::Px(150.0));
                s.height = Some(Length::Px(32.0));
            }
            "button" => {
                s.padding.top = 6.0;
                s.padding.bottom = 6.0;
                s.padding.left = 16.0;
                s.padding.right = 16.0;
                s.height = Some(Length::Px(32.0));
            }
            "textarea" => {
                s.padding.top = 6.0;
                s.padding.bottom = 6.0;
                s.padding.left = 8.0;
                s.padding.right = 8.0;
                s.width = Some(Length::Px(300.0));
                s.height = Some(Length::Px(80.0));
            }
            "img" => {
                s.width = Some(Length::Px(300.0));
                s.height = Some(Length::Px(200.0));
            }
            "audio" => {
                s.display = Display::Block;
                s.width = Some(Length::Px(300.0));
                s.height = Some(Length::Px(40.0));
            }
            "video" => {
                s.display = Display::Block;
                s.width = Some(Length::Px(480.0));
                s.height = Some(Length::Px(270.0));
            }
            "svg" => {
                // display: Block so children are stored as Block(children)
                // which paint_inline_svg can serialize back to SVG XML.
                s.display = Display::Block;
                // Read width/height from HTML attributes if present.
                // The actual values are set in apply_rules from CSS/attrs.
                // Default to 300x150 if no dimensions specified.
                s.width = Some(Length::Px(300.0));
                s.height = Some(Length::Px(150.0));
            }
            // SVG child elements — display: Block with 0 size so they appear
            // in the layout tree (needed by serialize_svg_children) but don't
            // affect the SVG element's dimensions.
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" | "text"
            | "g" | "defs" | "linearGradient" | "radialGradient" | "stop" | "use" | "symbol" => {
                s.display = Display::Block;
                s.width = Some(Length::Px(0.0));
                s.height = Some(Length::Px(0.0));
                s.margin = crate::style::EdgeSizes::default();
                s.padding = crate::style::EdgeSizes::default();
            }
            _ => {}
        }
        s
    }

    pub fn default_block() -> Self {
        let mut s = Self::default();
        s.display = Display::Block;
        s
    }
}

/// A node + its computed style + parent's computed style (for inheritance).
pub struct StyleTree<'a> {
    pub node: &'a Node,
    pub style: ComputedStyle,
    pub children: Vec<StyleTree<'a>>,
}

/// Build the style tree by walking the DOM and applying the cascade.
pub fn build_style_tree<'a>(
    root: &'a Node,
    stylesheet: &'a Stylesheet,
    animation_time_ms: u64,
) -> StyleTree<'a> {
    let matches = crate::css::collect_matches(stylesheet, root);
    // Index by Node pointer for quick lookup.
    let mut by_ptr: HashMap<*const Node, Vec<&Rule>> = HashMap::new();
    for (n, rules) in matches {
        let ptr: *const Node = n;
        by_ptr.insert(ptr, rules);
    }
    let default_style = ComputedStyle::default();
    build_recursive(root, &default_style, &by_ptr, stylesheet, animation_time_ms)
}

fn build_recursive<'a>(
    node: &'a Node,
    parent_style: &ComputedStyle,
    matches: &HashMap<*const Node, Vec<&'a Rule>>,
    stylesheet: &'a Stylesheet,
    animation_time_ms: u64,
) -> StyleTree<'a> {
    let (style, children) = match node {
        Node::Element(e) => {
            // Start from parent's inheritable properties (color, font, etc.).
            let mut style = ComputedStyle::default();
            style.color = parent_style.color;
            style.font_size = parent_style.font_size;
            style.font_weight = parent_style.font_weight;
            style.font_style = parent_style.font_style;
            style.font_family = parent_style.font_family.clone();
            style.line_height = parent_style.line_height;
            style.text_align = parent_style.text_align;
            // Apply UA styles on top — UA may override font_size (headings) etc.
            let ua = ComputedStyle::ua_for(&e.tag);
            style.display = ua.display;
            if ua.font_size != ComputedStyle::default().font_size {
                style.font_size = ua.font_size;
            }
            if ua.font_weight != ComputedStyle::default().font_weight {
                style.font_weight = ua.font_weight;
            }
            if ua.font_style != ComputedStyle::default().font_style {
                style.font_style = ua.font_style;
            }
            if ua.font_family != ComputedStyle::default().font_family {
                style.font_family = ua.font_family;
            }
            style.margin = ua.margin;
            style.padding = ua.padding;
            style.border = ua.border;
            // Apply matching rules in cascade order (later wins, except !important).
            let node_ptr: *const Node = node;
            if let Some(rules) = matches.get(&node_ptr) {
                apply_rules(&mut style, rules, parent_style);
            }
            // Inline `style` attribute overrides everything.
            if let Some(inline) = e.attrs.get("style") {
                let inline_sheet = crate::css::parse(&format!("* {{ {inline} }}"));
                let inline_rules: Vec<&Rule> = inline_sheet.rules.iter().collect();
                apply_rules(&mut style, &inline_rules, parent_style);
            }
            // For SVG elements, read width/height attributes from HTML.
            // These are presentation attributes in SVG — they should set
            // the CSS width/height if not already set by CSS.
            if e.tag == "svg" {
                // If this is a standalone SVG (data-falco-standalone attribute),
                // get width/height/viewBox from the global STANDALONE_SVG.
                if e.attrs.get("data-falco-standalone").is_some() {
                    if let Some(svg_xml) = crate::get_standalone_svg() {
                        if let Some(w) = extract_svg_attr(svg_xml, "width") {
                            if let Ok(px) = w.trim_end_matches("px").parse::<f32>() {
                                style.width = Some(Length::Px(px));
                            }
                        }
                        if let Some(h) = extract_svg_attr(svg_xml, "height") {
                            if let Ok(px) = h.trim_end_matches("px").parse::<f32>() {
                                style.height = Some(Length::Px(px));
                            }
                        }
                        if style.width.is_none() || style.height.is_none() {
                            if let Some(vb) = extract_svg_attr(svg_xml, "viewBox") {
                                let parts: Vec<&str> = vb.split_whitespace().collect();
                                if parts.len() == 4 {
                                    if let Ok(w) = parts[2].parse::<f32>() {
                                        if style.width.is_none() {
                                            style.width = Some(Length::Px(w));
                                        }
                                    }
                                    if let Ok(h) = parts[3].parse::<f32>() {
                                        if style.height.is_none() {
                                            style.height = Some(Length::Px(h));
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(original) = e.attrs.get("data-falco-original") {
                    let decoded = original.replace("&amp;", "&").replace("&quot;", "\"");
                    // Extract width/height/viewBox from the SVG tag.
                    if let Some(w) = extract_svg_attr(&decoded, "width") {
                        if let Ok(px) = w.trim_end_matches("px").parse::<f32>() {
                            style.width = Some(Length::Px(px));
                        }
                    }
                    if let Some(h) = extract_svg_attr(&decoded, "height") {
                        if let Ok(px) = h.trim_end_matches("px").parse::<f32>() {
                            style.height = Some(Length::Px(px));
                        }
                    }
                    if style.width.is_none() || style.height.is_none() {
                        if let Some(vb) = extract_svg_attr(&decoded, "viewBox") {
                            let parts: Vec<&str> = vb.split_whitespace().collect();
                            if parts.len() == 4 {
                                if let Ok(w) = parts[2].parse::<f32>() {
                                    if style.width.is_none() {
                                        style.width = Some(Length::Px(w));
                                    }
                                }
                                if let Ok(h) = parts[3].parse::<f32>() {
                                    if style.height.is_none() {
                                        style.height = Some(Length::Px(h));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Normal inline SVG — read width/height from HTML attributes.
                    if let Some(w) = e.attrs.get("width") {
                        if let Ok(px) = w.trim_end_matches("px").parse::<f32>() {
                            style.width = Some(Length::Px(px));
                        }
                    }
                    if let Some(h) = e.attrs.get("height") {
                        if let Ok(px) = h.trim_end_matches("px").parse::<f32>() {
                            style.height = Some(Length::Px(px));
                        }
                    }
                    if style.width.is_none() || style.height.is_none() {
                        if let Some(vb) = e.attrs.get("viewbox").or_else(|| e.attrs.get("viewBox"))
                        {
                            let parts: Vec<&str> = vb.split_whitespace().collect();
                            if parts.len() == 4 {
                                if let Ok(w) = parts[2].parse::<f32>() {
                                    if style.width.is_none() {
                                        style.width = Some(Length::Px(w));
                                    }
                                }
                                if let Ok(h) = parts[3].parse::<f32>() {
                                    if style.height.is_none() {
                                        style.height = Some(Length::Px(h));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // CSS animations: if the element has an `animation` property
            // referencing a @keyframes rule, interpolate the properties at
            // t=0.5 (50% through the animation) and apply them.
            //
            // This is a simplified animation: instead of running over time,
            // we evaluate at a fixed midpoint. This gives a visual
            // approximation in static PNG output.
            let anim_clone = style.animation.clone();
            if let Some(anim_str) = anim_clone {
                apply_keyframe_animation(&mut style, &anim_str, stylesheet, animation_time_ms);
            }
            let children = e
                .children
                .iter()
                .map(|c| build_recursive(c, &style, matches, stylesheet, animation_time_ms))
                .collect();
            (style, children)
        }
        Node::Document(d) => {
            let mut style = parent_style.clone();
            // Force document to be a block container.
            style.display = Display::Block;
            let children = d
                .children
                .iter()
                .map(|c| build_recursive(c, &style, matches, stylesheet, animation_time_ms))
                .collect();
            (style, children)
        }
        _ => {
            // Text, comment, doctype — inherit, no children to recurse into.
            (parent_style.clone(), Vec::new())
        }
    };
    StyleTree {
        node,
        style,
        children,
    }
}

/// Extract an attribute value from an SVG XML tag string.
/// e.g. extract_svg_attr("<svg width='100'>", "width") → Some("100")
fn extract_svg_attr(svg_xml: &str, attr: &str) -> Option<String> {
    // Look for attr="value" or attr='value'
    let lower = svg_xml.to_lowercase();
    let attr_lower = attr.to_lowercase();
    // Find the attribute in the first <svg ...> tag.
    let tag_end = lower.find('>').unwrap_or(lower.len());
    let tag = &lower[..tag_end];
    // Try double-quote.
    let pattern_dq = format!("{}=\"", attr_lower);
    if let Some(pos) = tag.find(&pattern_dq) {
        let start = pos + pattern_dq.len();
        if let Some(end) = tag[start..].find('"') {
            return Some(svg_xml[start..start + end].to_string());
        }
    }
    // Try single-quote.
    let pattern_sq = format!("{}='", attr_lower);
    if let Some(pos) = tag.find(&pattern_sq) {
        let start = pos + pattern_sq.len();
        if let Some(end) = tag[start..].find('\'') {
            return Some(svg_xml[start..start + end].to_string());
        }
    }
    None
}

/// Apply a CSS @keyframes animation to the computed style.
///
/// Linear interpolation between the two surrounding keyframe stops at t=0.5.
/// For opacity: interpolates numerically. For colors: interpolates each
/// RGB channel. For background-color: same as color.
fn apply_keyframe_animation(
    style: &mut ComputedStyle,
    anim_str: &str,
    stylesheet: &crate::css::Stylesheet,
    animation_time_ms: u64,
) {
    let anim_lower = anim_str.to_lowercase();
    for (name, kf) in &stylesheet.keyframes {
        if anim_lower.contains(&name.to_lowercase()) {
            let mut sorted_stops: Vec<&crate::css::KeyframeStop> = kf.stops.iter().collect();
            sorted_stops.sort_by(|a, b| {
                a.position
                    .partial_cmp(&b.position)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Parse animation-duration and iteration-count from the shorthand.
            let duration_s = parse_animation_duration(anim_str);
            let iteration_count = parse_animation_iteration_count(anim_str);

            // Compute animation progress (t) based on the current time.
            let time_s = animation_time_ms as f32 / 1000.0;
            let target = if duration_s > 0.0 {
                if iteration_count == 0.0 {
                    (time_s / duration_s) % 1.0
                } else {
                    let total = time_s / duration_s;
                    if total >= iteration_count {
                        1.0
                    } else {
                        total % 1.0
                    }
                }
            } else {
                0.0
            };

            let (before, after) = find_surrounding_stops(&sorted_stops, target);
            match (before, after) {
                (Some(b), Some(a)) => {
                    let range = a.position - b.position;
                    let t = if range > 0.0 {
                        (target - b.position) / range
                    } else {
                        0.0
                    };
                    interpolate_keyframe_properties(style, b, a, t);
                }
                (Some(b), None) => apply_stop_declarations(style, b),
                (None, Some(a)) => apply_stop_declarations(style, a),
                _ => {}
            }
            break;
        }
    }
}

/// Parse animation-duration (in seconds) from the animation shorthand string.
fn parse_animation_duration(anim_str: &str) -> f32 {
    let lower = anim_str.to_lowercase();
    if let Some(pos) = lower.find("ms") {
        let before = &lower[..pos];
        let num_str: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(n) = num_str.parse::<f32>() {
            return n / 1000.0;
        }
    }
    for i in 0..lower.len() {
        if lower.as_bytes().get(i) == Some(&b's') {
            if i > 0 && lower.as_bytes().get(i - 1) == Some(&b'm') {
                continue;
            }
            let before = &lower[..i];
            let num_str: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if let Ok(n) = num_str.parse::<f32>() {
                return n;
            }
        }
    }
    0.0
}

/// Parse animation-iteration-count from the shorthand.
/// Returns 0.0 for infinite, or the count (default 1.0).
fn parse_animation_iteration_count(anim_str: &str) -> f32 {
    let lower = anim_str.to_lowercase();
    if lower.contains("infinite") {
        return 0.0;
    }
    1.0
}

fn find_surrounding_stops<'a>(
    sorted: &[&'a crate::css::KeyframeStop],
    target: f32,
) -> (
    Option<&'a crate::css::KeyframeStop>,
    Option<&'a crate::css::KeyframeStop>,
) {
    let mut before = None;
    let mut after = None;
    for stop in sorted {
        if stop.position <= target {
            before = Some(*stop);
        }
        if stop.position >= target && after.is_none() {
            after = Some(*stop);
        }
    }
    (before, after)
}

fn apply_stop_declarations(style: &mut ComputedStyle, stop: &crate::css::KeyframeStop) {
    for decl in &stop.declarations {
        match decl.property.as_str() {
            "opacity" => {
                if let crate::css::Value::Number(n) = decl.value {
                    style.opacity = n;
                }
            }
            "color" => {
                if let crate::css::Value::Color(c) = decl.value {
                    style.color = c;
                }
            }
            "background-color" => {
                if let crate::css::Value::Color(c) = decl.value {
                    style.background_color = c;
                }
            }
            _ => {}
        }
    }
}

fn interpolate_keyframe_properties(
    style: &mut ComputedStyle,
    before: &crate::css::KeyframeStop,
    after: &crate::css::KeyframeStop,
    t: f32,
) {
    // Interpolate opacity.
    let before_opacity = before.declarations.iter().find(|d| d.property == "opacity");
    let after_opacity = after.declarations.iter().find(|d| d.property == "opacity");
    if let (Some(crate::css::Value::Number(b)), Some(crate::css::Value::Number(a))) = (
        before_opacity.map(|d| &d.value),
        after_opacity.map(|d| &d.value),
    ) {
        style.opacity = b + (a - b) * t;
    }
    // Interpolate color.
    let before_color = before.declarations.iter().find(|d| d.property == "color");
    let after_color = after.declarations.iter().find(|d| d.property == "color");
    if let (Some(crate::css::Value::Color(b)), Some(crate::css::Value::Color(a))) = (
        before_color.map(|d| &d.value),
        after_color.map(|d| &d.value),
    ) {
        style.color = interpolate_color(*b, *a, t);
    }
    // Interpolate background-color.
    let before_bg = before
        .declarations
        .iter()
        .find(|d| d.property == "background-color");
    let after_bg = after
        .declarations
        .iter()
        .find(|d| d.property == "background-color");
    if let (Some(crate::css::Value::Color(b)), Some(crate::css::Value::Color(a))) =
        (before_bg.map(|d| &d.value), after_bg.map(|d| &d.value))
    {
        style.background_color = interpolate_color(*b, *a, t);
    }
}

fn interpolate_color(b: crate::css::Color, a: crate::css::Color, t: f32) -> crate::css::Color {
    crate::css::Color::rgba(
        (b.r as f32 + (a.r as f32 - b.r as f32) * t).round() as u8,
        (b.g as f32 + (a.g as f32 - b.g as f32) * t).round() as u8,
        (b.b as f32 + (a.b as f32 - b.b as f32) * t).round() as u8,
        (b.a as f32 + (a.a as f32 - b.a as f32) * t).round() as u8,
    )
}

fn apply_rules(style: &mut ComputedStyle, rules: &[&Rule], parent: &ComputedStyle) {
    // First pass: normal declarations.
    for rule in rules {
        for decl in &rule.declarations {
            if !decl.important {
                apply_decl(style, decl, parent);
            }
        }
    }
    // Second pass: !important declarations.
    for rule in rules {
        for decl in &rule.declarations {
            if decl.important {
                apply_decl(style, decl, parent);
            }
        }
    }
}

fn apply_decl(style: &mut ComputedStyle, decl: &crate::css::Declaration, parent: &ComputedStyle) {
    let prop = decl.property.as_str();
    let val = &decl.value;
    match prop {
        "display" => {
            if let Value::Keyword(k) = val {
                style.display = match k.as_str() {
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "inline-block" => Display::InlineBlock,
                    "flex" => Display::Flex,
                    "inline-flex" => Display::InlineFlex,
                    "grid" => Display::Grid,
                    "inline-grid" => Display::InlineGrid,
                    "table" => Display::Table,
                    "table-row" => Display::TableRow,
                    "table-cell" => Display::TableCell,
                    "table-header-group" => Display::TableHeaderGroup,
                    "table-footer-group" => Display::TableFooterGroup,
                    "table-row-group" => Display::TableRowGroup,
                    "table-column-group" => Display::TableColumnGroup,
                    "table-column" => Display::TableColumn,
                    "table-caption" => Display::TableCaption,
                    "none" => Display::None,
                    _ => return,
                };
            }
        }
        "color" => {
            if let Value::Color(c) = val {
                style.color = *c;
            } else if let Value::Keyword(k) = val {
                if k == "inherit" {
                    style.color = parent.color;
                }
            }
        }
        "background" | "background-color" => {
            // Check for gradient in the value.
            let mut found_gradient = false;
            match val {
                Value::Keyword(k) => {
                    if k.starts_with("__linear-gradient__:")
                        || k.starts_with("__radial-gradient__:")
                    {
                        style.background_image = parse_background_image(val);
                        found_gradient = true;
                    }
                }
                Value::List(list) => {
                    for v in list {
                        match v {
                            Value::Color(c) => {
                                style.background_color = *c;
                            }
                            Value::Keyword(k)
                                if k.starts_with("__linear-gradient__:")
                                    || k.starts_with("__radial-gradient__:") =>
                            {
                                style.background_image = parse_background_image(v);
                                found_gradient = true;
                            }
                            Value::String(s) => {
                                style.background_image = Some(BackgroundImage::Url(s.clone()));
                            }
                            _ => {}
                        }
                    }
                }
                Value::Color(c) => {
                    style.background_color = *c;
                }
                Value::String(s) => {
                    style.background_image = Some(BackgroundImage::Url(s.clone()));
                }
                _ => {}
            }
            let _ = found_gradient;
        }
        "font-size" => {
            if let Value::Length(n, unit) = val {
                style.font_size = resolve_length_px(*n, *unit, parent.font_size);
            } else if let Value::Percentage(p) = val {
                style.font_size = parent.font_size * p / 100.0;
            } else if let Value::Keyword(k) = val {
                style.font_size = match k.as_str() {
                    "xx-small" => 9.0,
                    "x-small" => 10.0,
                    "small" => 13.0,
                    "medium" => 16.0,
                    "large" => 18.0,
                    "x-large" => 24.0,
                    "xx-large" => 32.0,
                    "smaller" => parent.font_size * 0.83,
                    "larger" => parent.font_size * 1.2,
                    _ => style.font_size,
                };
            }
        }
        "font-weight" => {
            if let Value::Number(n) = val {
                style.font_weight = *n as u16;
            } else if let Value::Keyword(k) = val {
                style.font_weight = match k.as_str() {
                    "normal" => 400,
                    "bold" => 700,
                    "bolder" => (parent.font_weight + 100).min(900),
                    "lighter" => (parent.font_weight - 100).max(100),
                    _ => style.font_weight,
                };
            }
        }
        "font-style" => {
            if let Value::Keyword(k) = val {
                style.font_style = match k.as_str() {
                    "italic" | "oblique" => FontStyle::Italic,
                    _ => FontStyle::Normal,
                };
            }
        }
        "font-family" => {
            if let Value::Keyword(k) = val {
                style.font_family = k.clone();
            } else if let Value::List(list) = val {
                // Take the first keyword.
                for v in list {
                    if let Value::Keyword(k) = v {
                        style.font_family = k.clone();
                        break;
                    }
                }
            }
        }
        "line-height" => {
            if let Value::Number(n) = val {
                style.line_height = *n;
            } else if let Value::Length(n, Unit::Px) = val {
                style.line_height = *n / style.font_size;
            } else if let Value::Percentage(p) = val {
                style.line_height = *p / 100.0;
            }
        }
        "text-align" => {
            if let Value::Keyword(k) = val {
                style.text_align = match k.as_str() {
                    "left" => TextAlign::Left,
                    "right" => TextAlign::Right,
                    "center" => TextAlign::Center,
                    "justify" => TextAlign::Justify,
                    _ => return,
                };
            }
        }
        // Shorthands.
        "margin" => {
            apply_edge_shorthand(&mut style.margin, val, parent);
        }
        "padding" => {
            apply_edge_shorthand(&mut style.padding, val, parent);
        }
        "border" => {
            apply_border_shorthand(&mut style.border, val, parent);
        }
        // Longhands.
        "margin-top" => {
            if let Some(v) = to_length(val, parent) {
                style.margin.top = v;
            }
        }
        "margin-right" => {
            if let Some(v) = to_length(val, parent) {
                style.margin.right = v;
            }
        }
        "margin-bottom" => {
            if let Some(v) = to_length(val, parent) {
                style.margin.bottom = v;
            }
        }
        "margin-left" => {
            if let Some(v) = to_length(val, parent) {
                style.margin.left = v;
            }
        }
        "padding-top" => {
            if let Some(v) = to_length(val, parent) {
                style.padding.top = v;
            }
        }
        "padding-right" => {
            if let Some(v) = to_length(val, parent) {
                style.padding.right = v;
            }
        }
        "padding-bottom" => {
            if let Some(v) = to_length(val, parent) {
                style.padding.bottom = v;
            }
        }
        "padding-left" => {
            if let Some(v) = to_length(val, parent) {
                style.padding.left = v;
            }
        }
        "border-width" => {
            let v = match val {
                Value::Length(n, Unit::Px) => Some(*n),
                _ => None,
            };
            if let Some(v) = v {
                style.border.top.width = v;
                style.border.bottom.width = v;
                style.border.left.width = v;
                style.border.right.width = v;
                if v > 0.0 {
                    if style.border.top.style == BorderStyle::None {
                        style.border.top.style = BorderStyle::Solid;
                    }
                    if style.border.bottom.style == BorderStyle::None {
                        style.border.bottom.style = BorderStyle::Solid;
                    }
                    if style.border.left.style == BorderStyle::None {
                        style.border.left.style = BorderStyle::Solid;
                    }
                    if style.border.right.style == BorderStyle::None {
                        style.border.right.style = BorderStyle::Solid;
                    }
                    if style.border.top.color == Color::TRANSPARENT {
                        style.border.top.color = style.color;
                    }
                    if style.border.bottom.color == Color::TRANSPARENT {
                        style.border.bottom.color = style.color;
                    }
                    if style.border.left.color == Color::TRANSPARENT {
                        style.border.left.color = style.color;
                    }
                    if style.border.right.color == Color::TRANSPARENT {
                        style.border.right.color = style.color;
                    }
                }
            }
        }
        "border-color" => {
            if let Value::Color(c) = val {
                style.border.top.color = *c;
                style.border.bottom.color = *c;
                style.border.left.color = *c;
                style.border.right.color = *c;
            }
        }
        "border-style" => {
            if let Value::Keyword(k) = val {
                let bs = match k.as_str() {
                    "solid" => BorderStyle::Solid,
                    "dotted" => BorderStyle::Dotted,
                    "dashed" => BorderStyle::Dashed,
                    _ => BorderStyle::None,
                };
                style.border.top.style = bs;
                style.border.bottom.style = bs;
                style.border.left.style = bs;
                style.border.right.style = bs;
            }
        }
        "border-top" => {
            apply_single_border(&mut style.border.top, val, parent, &style.color);
        }
        "border-right" => {
            apply_single_border(&mut style.border.right, val, parent, &style.color);
        }
        "border-bottom" => {
            apply_single_border(&mut style.border.bottom, val, parent, &style.color);
        }
        "border-left" => {
            apply_single_border(&mut style.border.left, val, parent, &style.color);
        }
        "width" => {
            style.width = to_length_opt(val, parent);
        }
        "height" => {
            style.height = to_length_opt(val, parent);
        }
        "min-width" => {
            style.min_width = to_length_opt(val, parent);
        }
        "max-width" => {
            style.max_width = to_length_opt(val, parent);
        }
        "position" => {
            if let Value::Keyword(k) = val {
                style.position = match k.as_str() {
                    "static" => Position::Static,
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    "fixed" => Position::Fixed,
                    _ => return,
                };
            }
        }
        "top" => {
            style.top = to_length_opt(val, parent);
        }
        "right" => {
            style.right = to_length_opt(val, parent);
        }
        "bottom" => {
            style.bottom = to_length_opt(val, parent);
        }
        "left" => {
            style.left = to_length_opt(val, parent);
        }
        "z-index" => {
            if let Value::Number(n) = val {
                style.z_index = Some(*n as i32);
            } else if let Value::Keyword(k) = val {
                if k == "auto" {
                    style.z_index = None;
                }
            }
        }
        "overflow" => {
            if let Value::Keyword(k) = val {
                style.overflow = match k.as_str() {
                    "hidden" => Overflow::Hidden,
                    "auto" => Overflow::Auto,
                    "scroll" => Overflow::Scroll,
                    _ => Overflow::Visible,
                };
            }
        }
        "opacity" => {
            if let Value::Number(n) = val {
                style.opacity = (*n).clamp(0.0, 1.0);
            }
        }
        "border-radius" => {
            let v = match val {
                Value::Length(n, Unit::Px) => *n,
                Value::Number(n) => *n,
                Value::Percentage(p) => *p,
                _ => return,
            };
            style.border_radius = BorderRadius {
                top_left: v,
                top_right: v,
                bottom_left: v,
                bottom_right: v,
            };
        }
        "border-top-left-radius" => {
            if let Some(v) = to_length(val, parent) {
                style.border_radius.top_left = v;
            }
        }
        "border-top-right-radius" => {
            if let Some(v) = to_length(val, parent) {
                style.border_radius.top_right = v;
            }
        }
        "border-bottom-left-radius" => {
            if let Some(v) = to_length(val, parent) {
                style.border_radius.bottom_left = v;
            }
        }
        "border-bottom-right-radius" => {
            if let Some(v) = to_length(val, parent) {
                style.border_radius.bottom_right = v;
            }
        }
        "box-shadow" => {
            style.box_shadow = parse_box_shadow(val);
        }
        "background-image" => {
            style.background_image = parse_background_image(val);
        }
        "flex-direction" => {
            if let Value::Keyword(k) = val {
                style.flex_direction = match k.as_str() {
                    "row" => FlexDirection::Row,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column" => FlexDirection::Column,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    _ => return,
                };
            }
        }
        "justify-content" => {
            if let Value::Keyword(k) = val {
                style.justify_content = match k.as_str() {
                    "flex-end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-evenly" => JustifyContent::SpaceEvenly,
                    _ => JustifyContent::FlexStart,
                };
            }
        }
        "align-items" => {
            if let Value::Keyword(k) = val {
                style.align_items = match k.as_str() {
                    "flex-start" => AlignItems::FlexStart,
                    "center" => AlignItems::Center,
                    "flex-end" => AlignItems::FlexEnd,
                    "baseline" => AlignItems::Baseline,
                    _ => AlignItems::Stretch,
                };
            }
        }
        "flex-wrap" => {
            if let Value::Keyword(k) = val {
                style.flex_wrap = match k.as_str() {
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => FlexWrap::Nowrap,
                };
            }
        }
        "gap" => {
            if let Value::Length(n, Unit::Px) = val {
                style.gap = *n;
            }
        }
        "flex-grow" => {
            if let Value::Number(n) = val {
                style.flex_grow = *n;
            }
        }
        "flex-shrink" => {
            if let Value::Number(n) = val {
                style.flex_shrink = *n;
            }
        }
        "flex-basis" => {
            style.flex_basis = to_length_opt(val, parent);
        }
        "flex" => {
            // flex: <grow> <shrink> <basis>
            match val {
                Value::Number(n) => {
                    style.flex_grow = *n;
                    style.flex_shrink = 1.0;
                    style.flex_basis = Some(Length::Px(0.0));
                }
                Value::Keyword(k) if k == "auto" => {
                    style.flex_grow = 1.0;
                    style.flex_shrink = 1.0;
                    style.flex_basis = Some(Length::Auto);
                }
                Value::Keyword(k) if k == "none" => {
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                    style.flex_basis = Some(Length::Auto);
                }
                Value::List(list) => {
                    let mut grow = 1.0;
                    let mut shrink = 1.0;
                    let mut basis: Option<Length> = None;
                    for v in list {
                        match v {
                            Value::Number(n) => {
                                if grow == 1.0 && basis.is_none() {
                                    grow = *n;
                                } else {
                                    shrink = *n;
                                }
                            }
                            Value::Length(n, unit) => {
                                basis = Some(match unit {
                                    Unit::Px => Length::Px(*n),
                                    Unit::Em => Length::Em(*n),
                                    _ => Length::Px(*n),
                                });
                            }
                            Value::Keyword(k) if k == "auto" => {
                                basis = Some(Length::Auto);
                            }
                            _ => {}
                        }
                    }
                    style.flex_grow = grow;
                    style.flex_shrink = shrink;
                    style.flex_basis = basis;
                }
                _ => {}
            }
        }
        "white-space" => {
            if let Value::Keyword(k) = val {
                style.white_space = match k.as_str() {
                    "pre" => WhiteSpace::Pre,
                    "pre-wrap" => WhiteSpace::PreWrap,
                    "nowrap" => WhiteSpace::Nowrap,
                    _ => WhiteSpace::Normal,
                };
            }
        }
        "box-sizing" => {
            if let Value::Keyword(k) = val {
                style.box_sizing = match k.as_str() {
                    "border-box" => BoxSizing::BorderBox,
                    _ => BoxSizing::ContentBox,
                };
            }
        }
        "gap" => {
            if let Value::Length(n, Unit::Px) = val {
                style.gap = *n;
            }
        }
        // ===== New v2 properties =====
        "float" => {
            if let Value::Keyword(k) = val {
                style.float = match k.as_str() {
                    "left" => Float::Left,
                    "right" => Float::Right,
                    "inline-start" => Float::InlineStart,
                    "inline-end" => Float::InlineEnd,
                    _ => Float::None,
                };
            }
        }
        "clear" => {
            if let Value::Keyword(k) = val {
                style.clear = match k.as_str() {
                    "left" => Clear::Left,
                    "right" => Clear::Right,
                    "both" => Clear::Both,
                    _ => Clear::None,
                };
            }
        }
        "object-fit" => {
            if let Value::Keyword(k) = val {
                style.object_fit = match k.as_str() {
                    "contain" => ObjectFit::Contain,
                    "cover" => ObjectFit::Cover,
                    "none" => ObjectFit::None,
                    "scale-down" => ObjectFit::ScaleDown,
                    _ => ObjectFit::Fill,
                };
            }
        }
        "transform" => {
            style.transform = parse_transform(val);
        }
        "vertical-align" => {
            if let Value::Keyword(k) = val {
                style.vertical_align = match k.as_str() {
                    "top" => VerticalAlign::Top,
                    "middle" => VerticalAlign::Middle,
                    "bottom" => VerticalAlign::Bottom,
                    "sub" => VerticalAlign::Sub,
                    "super" => VerticalAlign::Super,
                    "text-top" => VerticalAlign::TextTop,
                    "text-bottom" => VerticalAlign::TextBottom,
                    _ => VerticalAlign::Baseline,
                };
            }
        }
        "cursor" => {
            if let Value::Keyword(k) = val {
                style.cursor = match k.as_str() {
                    "pointer" => CursorType::Pointer,
                    "text" => CursorType::Text,
                    "wait" => CursorType::Wait,
                    "crosshair" => CursorType::Crosshair,
                    "not-allowed" => CursorType::NotAllowed,
                    "help" => CursorType::Help,
                    "move" => CursorType::Move,
                    "grab" => CursorType::Grab,
                    "default" => CursorType::Default,
                    _ => CursorType::Auto,
                };
            }
        }
        "text-decoration" => {
            if let Value::Keyword(k) = val {
                style.text_decoration = match k.as_str() {
                    "underline" => TextDecoration::Underline,
                    "line-through" => TextDecoration::LineThrough,
                    "overline" => TextDecoration::Overline,
                    _ => TextDecoration::None,
                };
            }
        }
        "text-transform" => {
            if let Value::Keyword(k) = val {
                style.text_transform = match k.as_str() {
                    "uppercase" => TextTransform::Uppercase,
                    "lowercase" => TextTransform::Lowercase,
                    "capitalize" => TextTransform::Capitalize,
                    "full-width" => TextTransform::FullWidth,
                    _ => TextTransform::None,
                };
            }
        }
        "letter-spacing" => {
            if let Value::Length(n, _) = val {
                style.letter_spacing = *n;
            }
        }
        "word-spacing" => {
            if let Value::Length(n, _) = val {
                style.word_spacing = *n;
            }
        }
        "list-style" | "list-style-type" => {
            if let Value::Keyword(k) = val {
                style.list_style_type = match k.as_str() {
                    "disc" => ListStyleType::Disc,
                    "circle" => ListStyleType::Circle,
                    "square" => ListStyleType::Square,
                    "decimal" => ListStyleType::Decimal,
                    "decimal-leading-zero" => ListStyleType::DecimalLeadingZero,
                    "lower-alpha" | "lower-latin" => ListStyleType::LowerAlpha,
                    "upper-alpha" | "upper-latin" => ListStyleType::UpperAlpha,
                    "lower-roman" => ListStyleType::LowerRoman,
                    "upper-roman" => ListStyleType::UpperRoman,
                    "none" => ListStyleType::None,
                    _ => ListStyleType::Disc,
                };
            }
        }
        "table-layout" => {
            if let Value::Keyword(k) = val {
                style.table_layout = match k.as_str() {
                    "fixed" => TableLayout::Fixed,
                    _ => TableLayout::Auto,
                };
            }
        }
        "border-collapse" => {
            if let Value::Keyword(k) = val {
                style.border_collapse = match k.as_str() {
                    "collapse" => BorderCollapse::Collapse,
                    _ => BorderCollapse::Separate,
                };
            }
        }
        "visibility" => {
            if let Value::Keyword(k) = val {
                style.visibility = match k.as_str() {
                    "hidden" => Visibility::Hidden,
                    "collapse" => Visibility::Collapse,
                    _ => Visibility::Visible,
                };
            }
        }
        "outline-width" => {
            if let Value::Length(n, Unit::Px) = val {
                style.outline_width = *n;
            }
        }
        "outline-color" => {
            if let Value::Color(c) = val {
                style.outline_color = *c;
            }
        }
        "outline-style" => {
            if let Value::Keyword(k) = val {
                style.outline_style = match k.as_str() {
                    "solid" => BorderStyle::Solid,
                    "dotted" => BorderStyle::Dotted,
                    "dashed" => BorderStyle::Dashed,
                    _ => BorderStyle::None,
                };
            }
        }
        "animation" => {
            style.animation = Some(format!("{:?}", val));
        }
        "transition" => {
            style.transition = Some(format!("{:?}", val));
        }
        "outline" => {
            // outline: <width> <style> <color>
            let values: Vec<&Value> = match val {
                Value::List(list) => list.iter().collect(),
                _ => vec![val],
            };
            for v in values {
                match v {
                    Value::Length(n, Unit::Px) => style.outline_width = *n,
                    Value::Color(c) => style.outline_color = *c,
                    Value::Keyword(k) => {
                        style.outline_style = match k.as_str() {
                            "solid" => BorderStyle::Solid,
                            "dotted" => BorderStyle::Dotted,
                            "dashed" => BorderStyle::Dashed,
                            "none" => BorderStyle::None,
                            _ => continue,
                        };
                    }
                    _ => {}
                }
            }
        }
        _ => {
            // CSS custom properties: --foo
            if prop.starts_with("--") {
                let val_str = match val {
                    Value::Keyword(k) => k.clone(),
                    Value::String(s) => s.clone(),
                    _ => format!("{:?}", val),
                };
                style.custom_properties.insert(prop.to_string(), val_str);
            } else {
                // Store ALL unrecognized properties in property_store
                // (grid-template-columns, grid-column, etc.)
                let val_str = match val {
                    Value::Keyword(k) => k.clone(),
                    Value::String(s) => s.clone(),
                    Value::Length(n, Unit::Px) => format!("{}px", n),
                    Value::Length(n, Unit::Em) => format!("{}em", n),
                    Value::Length(n, Unit::Rem) => format!("{}rem", n),
                    Value::Length(n, Unit::Percent) => format!("{}%", n),
                    Value::Length(n, Unit::Vw) => format!("{}vw", n),
                    Value::Length(n, Unit::Vh) => format!("{}vh", n),
                    Value::Length(n, Unit::Pt) => format!("{}pt", n),
                    Value::Color(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
                    Value::Number(n) => n.to_string(),
                    Value::Percentage(p) => format!("{}%", p),
                    Value::List(list) => list
                        .iter()
                        .map(|v| match v {
                            Value::Keyword(k) => k.clone(),
                            Value::String(s) => s.clone(),
                            Value::Length(n, Unit::Px) => format!("{}px", n),
                            Value::Number(n) => n.to_string(),
                            Value::Percentage(p) => format!("{}%", p),
                            _ => format!("{:?}", v),
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => format!("{:?}", val),
                };
                style.property_store.insert(prop.to_string(), val_str);
            }
        }
    }
}

/// Parse a CSS transform value into Transform operations.
fn parse_transform(val: &Value) -> Option<Transform> {
    let k = match val {
        Value::Keyword(k) => k.as_str(),
        _ => return None,
    };
    // Transforms are stored as __function__:args keywords by the CSS parser.
    // For now, handle simple cases.
    if k == "none" {
        return None;
    }
    // Try to parse translate(x, y), rotate(deg), scale(x, y)
    // The CSS parser stores these as keywords like "__translate__:10px,20px"
    if let Some(args) = k.strip_prefix("__translate__:") {
        let parts: Vec<&str> = args.split(',').collect();
        let x = parts.first().and_then(|s| parse_px(s)).unwrap_or(0.0);
        let y = parts.get(1).and_then(|s| parse_px(s)).unwrap_or(0.0);
        return Some(Transform::Translate(x, y));
    }
    if let Some(args) = k.strip_prefix("__rotate__:") {
        let deg = args.trim_end_matches("deg").trim().parse::<f32>().ok()?;
        return Some(Transform::Rotate(deg));
    }
    if let Some(args) = k.strip_prefix("__scale__:") {
        let parts: Vec<&str> = args.split(',').collect();
        let x = parts
            .first()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        let y = parts
            .get(1)
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(x);
        return Some(Transform::Scale(x, y));
    }
    None
}

fn parse_px(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").trim().parse::<f32>().ok()
}

fn apply_edge_shorthand(edges: &mut EdgeSizes, val: &Value, parent: &ComputedStyle) {
    let values: Vec<&Value> = match val {
        Value::List(list) => list.iter().collect(),
        _ => vec![val],
    };
    let mut nums: Vec<f32> = Vec::new();
    for v in &values {
        if let Some(n) = to_length(v, parent) {
            nums.push(n);
        }
    }
    match nums.len() {
        1 => {
            edges.top = nums[0];
            edges.right = nums[0];
            edges.bottom = nums[0];
            edges.left = nums[0];
        }
        2 => {
            edges.top = nums[0];
            edges.right = nums[1];
            edges.bottom = nums[0];
            edges.left = nums[1];
        }
        3 => {
            edges.top = nums[0];
            edges.right = nums[1];
            edges.bottom = nums[2];
            edges.left = nums[1];
        }
        4 => {
            edges.top = nums[0];
            edges.right = nums[1];
            edges.bottom = nums[2];
            edges.left = nums[3];
        }
        _ => {}
    }
}

fn apply_border_shorthand(borders: &mut BorderEdges, val: &Value, parent: &ComputedStyle) {
    // border: <width> <style> <color>
    let mut width = 3.0;
    let mut style = BorderStyle::Solid;
    let mut color = Color::rgb(0, 0, 0);
    let values: Vec<&Value> = match val {
        Value::List(list) => list.iter().collect(),
        _ => vec![val],
    };
    for v in values {
        match v {
            Value::Length(n, Unit::Px) => width = *n,
            Value::Color(c) => color = *c,
            Value::Keyword(k) => match k.as_str() {
                "solid" => style = BorderStyle::Solid,
                "dotted" => style = BorderStyle::Dotted,
                "dashed" => style = BorderStyle::Dashed,
                "none" => style = BorderStyle::None,
                _ => {}
            },
            _ => {}
        }
    }
    borders.top = BorderEdge {
        width,
        color,
        style,
    };
    borders.bottom = BorderEdge {
        width,
        color,
        style,
    };
    borders.left = BorderEdge {
        width,
        color,
        style,
    };
    borders.right = BorderEdge {
        width,
        color,
        style,
    };
    let _ = parent;
}

fn apply_single_border(
    edge: &mut BorderEdge,
    val: &Value,
    _parent: &ComputedStyle,
    _inherit_color: &Color,
) {
    let values: Vec<&Value> = match val {
        Value::List(list) => list.iter().collect(),
        _ => vec![val],
    };
    for v in values {
        match v {
            Value::Length(n, Unit::Px) => edge.width = *n,
            Value::Color(c) => edge.color = *c,
            Value::Keyword(k) => match k.as_str() {
                "solid" => edge.style = BorderStyle::Solid,
                "dotted" => edge.style = BorderStyle::Dotted,
                "dashed" => edge.style = BorderStyle::Dashed,
                "none" => edge.style = BorderStyle::None,
                _ => {}
            },
            _ => {}
        }
    }
}

fn to_length(val: &Value, parent: &ComputedStyle) -> Option<f32> {
    match val {
        Value::Length(n, unit) => Some(resolve_length_px(*n, *unit, parent.font_size)),
        Value::Percentage(p) => Some(*p), // caller resolves against parent width
        Value::Keyword(k) if k == "auto" => Some(0.0),
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

fn to_length_opt(val: &Value, _parent: &ComputedStyle) -> Option<Length> {
    match val {
        Value::Length(n, unit) => Some(match unit {
            Unit::Px => Length::Px(*n),
            Unit::Em => Length::Em(*n),
            Unit::Rem => Length::Em(*n),
            Unit::Pt => Length::Px(*n * 1.333),
            Unit::Vw => Length::Vw(*n),
            Unit::Vh => Length::Vh(*n),
            Unit::Percent => Length::Percent(*n),
        }),
        Value::Percentage(p) => Some(Length::Percent(*p)),
        Value::Keyword(k) if k == "auto" => Some(Length::Auto),
        _ => None,
    }
}

fn resolve_length_px(n: f32, unit: Unit, parent_font_size: f32) -> f32 {
    match unit {
        Unit::Px => n,
        Unit::Em | Unit::Rem => n * parent_font_size,
        Unit::Pt => n * 1.333,
        Unit::Vw | Unit::Vh => n * 0.5, // resolved later with viewport size; default 50%
        Unit::Percent => n,
    }
}

/// Parse box-shadow value: <offset-x> <offset-y> <blur> <spread> <color> [inset]
fn parse_box_shadow(val: &Value) -> Vec<BoxShadow> {
    let shadows: Vec<BoxShadow> = Vec::new();
    match val {
        Value::Keyword(k) if k == "none" => return shadows,
        Value::List(list) => {
            let mut ox = 0.0;
            let mut oy = 0.0;
            let mut blur = 0.0;
            let mut spread = 0.0;
            let mut color = Color::rgb(0, 0, 0);
            let mut inset = false;
            let mut num_count = 0;
            for v in list {
                match v {
                    Value::Length(n, Unit::Px) => match num_count {
                        0 => {
                            ox = *n;
                            num_count = 1;
                        }
                        1 => {
                            oy = *n;
                            num_count = 2;
                        }
                        2 => {
                            blur = *n;
                            num_count = 3;
                        }
                        3 => {
                            spread = *n;
                            num_count = 4;
                        }
                        _ => {}
                    },
                    Value::Color(c) => {
                        color = *c;
                    }
                    Value::Keyword(k) if k == "inset" => {
                        inset = true;
                    }
                    _ => {}
                }
            }
            return vec![BoxShadow {
                offset_x: ox,
                offset_y: oy,
                blur,
                spread,
                color,
                inset,
            }];
        }
        _ => {}
    }
    shadows
}

/// Parse background-image value (linear-gradient or url).
fn parse_background_image(val: &Value) -> Option<BackgroundImage> {
    match val {
        Value::Keyword(k) => {
            if let Some(args) = k.strip_prefix("__linear-gradient__:") {
                return parse_linear_gradient(args);
            }
            if let Some(args) = k.strip_prefix("__radial-gradient__:") {
                // Simplify radial to linear for now.
                return parse_linear_gradient(args);
            }
            None
        }
        Value::String(s) => Some(BackgroundImage::Url(s.clone())),
        _ => None,
    }
}

/// Parse linear-gradient args: "135deg, #ff0000, #0000ff" or "to right, #ff0000, #0000ff"
fn parse_linear_gradient(args: &str) -> Option<BackgroundImage> {
    let parts: Vec<&str> = args.split(',').collect();
    if parts.is_empty() {
        return None;
    }

    let mut angle = 180.0; // default: top to bottom
    let mut color_start = 0;

    // First part might be an angle or direction.
    let first = parts[0].trim();
    if first.ends_with("deg") {
        angle = first.trim_end_matches("deg").trim().parse::<f32>().ok()?;
        color_start = 1;
    } else if first.starts_with("to ") {
        angle = match first {
            "to top" => 0.0,
            "to right" => 90.0,
            "to bottom" => 180.0,
            "to left" => 270.0,
            "to top right" | "to right top" => 45.0,
            "to bottom right" | "to right bottom" => 135.0,
            "to bottom left" | "to left bottom" => 225.0,
            "to top left" | "to left top" => 315.0,
            _ => 180.0,
        };
        color_start = 1;
    }

    let mut stops: Vec<(f32, Color)> = Vec::new();
    let color_parts = &parts[color_start..];
    let n = color_parts.len() as f32;
    for (i, part) in color_parts.iter().enumerate() {
        let part = part.trim();
        // Stop might be "color position" or just "color".
        let sub_parts: Vec<&str> = part.split_whitespace().collect();
        if let Some(c) = crate::css::parse_color(sub_parts[0]) {
            let pos = if sub_parts.len() > 1 {
                sub_parts[1]
                    .trim_end_matches('%')
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|p| p / 100.0)
            } else {
                None
            }
            .unwrap_or(if n > 1.0 { i as f32 / (n - 1.0) } else { 0.0 });
            stops.push((pos, c));
        }
    }

    if stops.is_empty() {
        return None;
    }
    Some(BackgroundImage::LinearGradient { angle, stops })
}
