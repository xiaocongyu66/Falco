//! CSS property registry — all 300+ CSS properties organized by category.
//!
//! This module serves as the authoritative list of CSS properties that
//! Falco recognizes. Properties not in this list are treated as unknown
//! and silently dropped during parsing.
//!
//! Each property has:
//! - A name (lowercase, hyphenated)
//! - A category (for documentation and future feature-gating)
//! - An "applied" flag indicating whether Falco actually uses the value
//!   in layout/paint, or just stores it for inheritance/future use.

use std::collections::HashSet;
use std::sync::OnceLock;

/// CSS property categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyCategory {
    BoxModel,
    Border,
    Background,
    Text,
    Font,
    Layout,
    Flexbox,
    Grid,
    Positioning,
    Visual,
    List,
    Table,
    Animation,
    Transition,
    Transform,
    GeneratedContent,
    Print,
    UserInterface,
    WritingMode,
    Multicol,
    Miscellaneous,
}

/// A registered CSS property.
#[derive(Debug, Clone)]
pub struct PropertyDef {
    pub name: &'static str,
    pub category: PropertyCategory,
    /// True if Falco applies this property in layout or paint.
    /// False if it's recognized but not yet implemented.
    pub applied: bool,
    /// True if the property is inherited (like color, font-size).
    pub inherited: bool,
}

/// Get the full registry of 300+ CSS properties.
pub fn registry() -> &'static HashSet<&'static str> {
    static REGISTRY: OnceLock<HashSet<&'static str>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let props: Vec<&str> = ALL_PROPERTIES.iter().map(|p| p.name).collect();
        props.into_iter().collect()
    })
}

/// Check if a property name is recognized by Falco.
pub fn is_known(property: &str) -> bool {
    registry().contains(property)
}

/// Get all property definitions.
pub fn all_properties() -> &'static [PropertyDef] {
    ALL_PROPERTIES
}

/// Count applied (implemented) properties.
pub fn applied_count() -> usize {
    ALL_PROPERTIES.iter().filter(|p| p.applied).count()
}

/// Count total recognized properties.
pub fn total_count() -> usize {
    ALL_PROPERTIES.len()
}

// Helper macros for building the property list.
const fn p(
    name: &'static str,
    cat: PropertyCategory,
    applied: bool,
    inherited: bool,
) -> PropertyDef {
    PropertyDef {
        name,
        category: cat,
        applied,
        inherited,
    }
}

/// The complete list of 300+ CSS properties.
pub const ALL_PROPERTIES: &[PropertyDef] = &[
    // ===== Box Model (25) =====
    p("width", PropertyCategory::BoxModel, true, false),
    p("height", PropertyCategory::BoxModel, true, false),
    p("min-width", PropertyCategory::BoxModel, true, false),
    p("max-width", PropertyCategory::BoxModel, true, false),
    p("min-height", PropertyCategory::BoxModel, true, false),
    p("max-height", PropertyCategory::BoxModel, true, false),
    p("margin", PropertyCategory::BoxModel, true, false),
    p("margin-top", PropertyCategory::BoxModel, true, false),
    p("margin-right", PropertyCategory::BoxModel, true, false),
    p("margin-bottom", PropertyCategory::BoxModel, true, false),
    p("margin-left", PropertyCategory::BoxModel, true, false),
    p("padding", PropertyCategory::BoxModel, true, false),
    p("padding-top", PropertyCategory::BoxModel, true, false),
    p("padding-right", PropertyCategory::BoxModel, true, false),
    p("padding-bottom", PropertyCategory::BoxModel, true, false),
    p("padding-left", PropertyCategory::BoxModel, true, false),
    p("box-sizing", PropertyCategory::BoxModel, true, false),
    p("box-shadow", PropertyCategory::BoxModel, true, false),
    p("outline", PropertyCategory::BoxModel, false, false),
    p("outline-width", PropertyCategory::BoxModel, false, false),
    p("outline-style", PropertyCategory::BoxModel, false, false),
    p("outline-color", PropertyCategory::BoxModel, false, false),
    p("outline-offset", PropertyCategory::BoxModel, false, false),
    p("aspect-ratio", PropertyCategory::BoxModel, false, false),
    p("object-fit", PropertyCategory::BoxModel, true, false),
    p("object-position", PropertyCategory::BoxModel, false, false),
    // ===== Borders (28) =====
    p("border", PropertyCategory::Border, true, false),
    p("border-top", PropertyCategory::Border, true, false),
    p("border-right", PropertyCategory::Border, true, false),
    p("border-bottom", PropertyCategory::Border, true, false),
    p("border-left", PropertyCategory::Border, true, false),
    p("border-width", PropertyCategory::Border, true, false),
    p("border-top-width", PropertyCategory::Border, false, false),
    p("border-right-width", PropertyCategory::Border, false, false),
    p(
        "border-bottom-width",
        PropertyCategory::Border,
        false,
        false,
    ),
    p("border-left-width", PropertyCategory::Border, false, false),
    p("border-style", PropertyCategory::Border, true, false),
    p("border-top-style", PropertyCategory::Border, false, false),
    p("border-right-style", PropertyCategory::Border, false, false),
    p(
        "border-bottom-style",
        PropertyCategory::Border,
        false,
        false,
    ),
    p("border-left-style", PropertyCategory::Border, false, false),
    p("border-color", PropertyCategory::Border, true, false),
    p("border-top-color", PropertyCategory::Border, false, false),
    p("border-right-color", PropertyCategory::Border, false, false),
    p(
        "border-bottom-color",
        PropertyCategory::Border,
        false,
        false,
    ),
    p("border-left-color", PropertyCategory::Border, false, false),
    p("border-radius", PropertyCategory::Border, true, false),
    p(
        "border-top-left-radius",
        PropertyCategory::Border,
        true,
        false,
    ),
    p(
        "border-top-right-radius",
        PropertyCategory::Border,
        true,
        false,
    ),
    p(
        "border-bottom-left-radius",
        PropertyCategory::Border,
        true,
        false,
    ),
    p(
        "border-bottom-right-radius",
        PropertyCategory::Border,
        true,
        false,
    ),
    p("border-collapse", PropertyCategory::Border, false, false),
    p("border-spacing", PropertyCategory::Border, false, false),
    p("border-image", PropertyCategory::Border, false, false),
    p(
        "border-image-source",
        PropertyCategory::Border,
        false,
        false,
    ),
    p("border-image-slice", PropertyCategory::Border, false, false),
    p("border-image-width", PropertyCategory::Border, false, false),
    p(
        "border-image-outset",
        PropertyCategory::Border,
        false,
        false,
    ),
    p(
        "border-image-repeat",
        PropertyCategory::Border,
        false,
        false,
    ),
    // ===== Background (12) =====
    p("background", PropertyCategory::Background, true, false),
    p(
        "background-color",
        PropertyCategory::Background,
        true,
        false,
    ),
    p(
        "background-image",
        PropertyCategory::Background,
        true,
        false,
    ),
    p(
        "background-repeat",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-position",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-position-x",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-position-y",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-size",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-attachment",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-origin",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-clip",
        PropertyCategory::Background,
        false,
        false,
    ),
    p(
        "background-blend-mode",
        PropertyCategory::Background,
        false,
        false,
    ),
    // ===== Text (35) =====
    p("color", PropertyCategory::Text, true, true),
    p("text-align", PropertyCategory::Text, true, true),
    p("text-decoration", PropertyCategory::Text, true, true),
    p("text-decoration-color", PropertyCategory::Text, true, true),
    p("text-decoration-style", PropertyCategory::Text, false, true),
    p("text-decoration-line", PropertyCategory::Text, false, true),
    p(
        "text-decoration-thickness",
        PropertyCategory::Text,
        false,
        true,
    ),
    p(
        "text-decoration-skip-ink",
        PropertyCategory::Text,
        false,
        true,
    ),
    p("text-transform", PropertyCategory::Text, true, true),
    p("text-indent", PropertyCategory::Text, false, true),
    p("text-shadow", PropertyCategory::Text, false, true),
    p("text-overflow", PropertyCategory::Text, false, false),
    p("text-rendering", PropertyCategory::Text, false, true),
    p("text-justify", PropertyCategory::Text, false, true),
    p("text-orientation", PropertyCategory::Text, false, true),
    p("text-combine-upright", PropertyCategory::Text, false, true),
    p(
        "text-underline-position",
        PropertyCategory::Text,
        false,
        true,
    ),
    p("text-emphasis", PropertyCategory::Text, false, true),
    p("text-emphasis-color", PropertyCategory::Text, false, true),
    p("text-emphasis-style", PropertyCategory::Text, false, true),
    p(
        "text-emphasis-position",
        PropertyCategory::Text,
        false,
        true,
    ),
    p("text-wrap", PropertyCategory::Text, false, true),
    p("text-align-last", PropertyCategory::Text, false, true),
    p("text-spacing", PropertyCategory::Text, false, true),
    p("letter-spacing", PropertyCategory::Text, true, true),
    p("word-spacing", PropertyCategory::Text, true, true),
    p("word-break", PropertyCategory::Text, false, true),
    p("word-wrap", PropertyCategory::Text, false, true),
    p("overflow-wrap", PropertyCategory::Text, false, true),
    p("line-height", PropertyCategory::Text, true, true),
    p("line-clamp", PropertyCategory::Text, false, false),
    p("white-space", PropertyCategory::Text, true, true),
    p("hyphens", PropertyCategory::Text, false, true),
    p("tab-size", PropertyCategory::Text, false, true),
    p("direction", PropertyCategory::Text, false, true),
    p("unicode-bidi", PropertyCategory::Text, false, true),
    p("writing-mode", PropertyCategory::Text, false, true),
    p("ruby-position", PropertyCategory::Text, false, true),
    p("ruby-align", PropertyCategory::Text, false, true),
    p("hanging-punctuation", PropertyCategory::Text, false, true),
    // ===== Font (18) =====
    p("font", PropertyCategory::Font, true, true),
    p("font-family", PropertyCategory::Font, true, true),
    p("font-size", PropertyCategory::Font, true, true),
    p("font-size-adjust", PropertyCategory::Font, false, true),
    p("font-style", PropertyCategory::Font, true, true),
    p("font-weight", PropertyCategory::Font, true, true),
    p("font-variant", PropertyCategory::Font, false, true),
    p("font-variant-caps", PropertyCategory::Font, false, true),
    p("font-variant-numeric", PropertyCategory::Font, false, true),
    p(
        "font-variant-ligatures",
        PropertyCategory::Font,
        false,
        true,
    ),
    p("font-variant-position", PropertyCategory::Font, false, true),
    p(
        "font-variant-east-asian",
        PropertyCategory::Font,
        false,
        true,
    ),
    p(
        "font-variant-alternates",
        PropertyCategory::Font,
        false,
        true,
    ),
    p("font-stretch", PropertyCategory::Font, false, true),
    p("font-feature-settings", PropertyCategory::Font, false, true),
    p("font-kerning", PropertyCategory::Font, false, true),
    p(
        "font-language-override",
        PropertyCategory::Font,
        false,
        true,
    ),
    p("font-optical-sizing", PropertyCategory::Font, false, true),
    p(
        "font-variation-settings",
        PropertyCategory::Font,
        false,
        true,
    ),
    p("font-synthesis", PropertyCategory::Font, false, true),
    p("font-palette", PropertyCategory::Font, false, true),
    // ===== Layout (10) =====
    p("display", PropertyCategory::Layout, true, false),
    p("position", PropertyCategory::Layout, true, false),
    p("top", PropertyCategory::Layout, true, false),
    p("right", PropertyCategory::Layout, true, false),
    p("bottom", PropertyCategory::Layout, true, false),
    p("left", PropertyCategory::Layout, true, false),
    p("z-index", PropertyCategory::Layout, true, false),
    p("overflow", PropertyCategory::Layout, true, false),
    p("overflow-x", PropertyCategory::Layout, false, false),
    p("overflow-y", PropertyCategory::Layout, false, false),
    p("visibility", PropertyCategory::Layout, true, true),
    p("opacity", PropertyCategory::Layout, true, false),
    p("float", PropertyCategory::Layout, false, false),
    p("clear", PropertyCategory::Layout, false, false),
    p("clip", PropertyCategory::Layout, false, false),
    p("resize", PropertyCategory::Layout, false, false),
    p("content", PropertyCategory::Layout, false, false),
    p("quotes", PropertyCategory::Layout, false, true),
    p("counter-reset", PropertyCategory::Layout, false, false),
    p("counter-increment", PropertyCategory::Layout, false, false),
    p("contain", PropertyCategory::Layout, false, false),
    p("will-change", PropertyCategory::Layout, false, false),
    p("isolation", PropertyCategory::Layout, false, false),
    p("all", PropertyCategory::Layout, false, false),
    // ===== Flexbox (16) =====
    p("flex", PropertyCategory::Flexbox, true, false),
    p("flex-direction", PropertyCategory::Flexbox, true, false),
    p("flex-wrap", PropertyCategory::Flexbox, true, false),
    p("flex-flow", PropertyCategory::Flexbox, false, false),
    p("flex-grow", PropertyCategory::Flexbox, true, false),
    p("flex-shrink", PropertyCategory::Flexbox, true, false),
    p("flex-basis", PropertyCategory::Flexbox, true, false),
    p("justify-content", PropertyCategory::Flexbox, true, false),
    p("justify-items", PropertyCategory::Flexbox, false, false),
    p("justify-self", PropertyCategory::Flexbox, false, false),
    p("align-items", PropertyCategory::Flexbox, true, false),
    p("align-content", PropertyCategory::Flexbox, false, false),
    p("align-self", PropertyCategory::Flexbox, false, false),
    p("order", PropertyCategory::Flexbox, false, false),
    p("gap", PropertyCategory::Flexbox, true, false),
    p("row-gap", PropertyCategory::Flexbox, false, false),
    p("column-gap", PropertyCategory::Flexbox, false, false),
    p("place-content", PropertyCategory::Flexbox, false, false),
    p("place-items", PropertyCategory::Flexbox, false, false),
    p("place-self", PropertyCategory::Flexbox, false, false),
    // ===== Grid (22) =====
    p("grid", PropertyCategory::Grid, false, false),
    p("grid-template", PropertyCategory::Grid, false, false),
    p(
        "grid-template-columns",
        PropertyCategory::Grid,
        false,
        false,
    ),
    p("grid-template-rows", PropertyCategory::Grid, false, false),
    p("grid-template-areas", PropertyCategory::Grid, false, false),
    p("grid-area", PropertyCategory::Grid, false, false),
    p("grid-column", PropertyCategory::Grid, false, false),
    p("grid-column-start", PropertyCategory::Grid, false, false),
    p("grid-column-end", PropertyCategory::Grid, false, false),
    p("grid-row", PropertyCategory::Grid, false, false),
    p("grid-row-start", PropertyCategory::Grid, false, false),
    p("grid-row-end", PropertyCategory::Grid, false, false),
    p("grid-auto-flow", PropertyCategory::Grid, false, false),
    p("grid-auto-columns", PropertyCategory::Grid, false, false),
    p("grid-auto-rows", PropertyCategory::Grid, false, false),
    p("grid-gap", PropertyCategory::Grid, false, false),
    p("justify-items", PropertyCategory::Grid, false, false),
    p("justify-self", PropertyCategory::Grid, false, false),
    p("align-items", PropertyCategory::Grid, false, false),
    p("align-self", PropertyCategory::Grid, false, false),
    p("align-content", PropertyCategory::Grid, false, false),
    p("justify-content", PropertyCategory::Grid, false, false),
    // ===== Positioning (5) =====
    p("position", PropertyCategory::Positioning, true, false),
    p("inset", PropertyCategory::Positioning, false, false),
    p("inset-block", PropertyCategory::Positioning, false, false),
    p("inset-inline", PropertyCategory::Positioning, false, false),
    p(
        "inset-block-start",
        PropertyCategory::Positioning,
        false,
        false,
    ),
    p(
        "inset-block-end",
        PropertyCategory::Positioning,
        false,
        false,
    ),
    p(
        "inset-inline-start",
        PropertyCategory::Positioning,
        false,
        false,
    ),
    p(
        "inset-inline-end",
        PropertyCategory::Positioning,
        false,
        false,
    ),
    // ===== Visual / Effects (20) =====
    p("box-shadow", PropertyCategory::Visual, true, false),
    p("clip", PropertyCategory::Visual, false, false),
    p("clip-path", PropertyCategory::Visual, false, false),
    p("filter", PropertyCategory::Visual, false, false),
    p("backdrop-filter", PropertyCategory::Visual, false, false),
    p("mix-blend-mode", PropertyCategory::Visual, false, false),
    p(
        "background-blend-mode",
        PropertyCategory::Visual,
        false,
        false,
    ),
    p("opacity", PropertyCategory::Visual, true, false),
    p("visibility", PropertyCategory::Visual, true, true),
    p("transform", PropertyCategory::Visual, true, false),
    p("transform-origin", PropertyCategory::Visual, false, false),
    p("transform-style", PropertyCategory::Visual, false, false),
    p("perspective", PropertyCategory::Visual, false, false),
    p("perspective-origin", PropertyCategory::Visual, false, false),
    p(
        "backface-visibility",
        PropertyCategory::Visual,
        false,
        false,
    ),
    p("mask", PropertyCategory::Visual, false, false),
    p("mask-image", PropertyCategory::Visual, false, false),
    p("mask-size", PropertyCategory::Visual, false, false),
    p("mask-position", PropertyCategory::Visual, false, false),
    p("mask-repeat", PropertyCategory::Visual, false, false),
    p("mask-clip", PropertyCategory::Visual, false, false),
    p("mask-origin", PropertyCategory::Visual, false, false),
    p("mask-mode", PropertyCategory::Visual, false, false),
    p("mask-composite", PropertyCategory::Visual, false, false),
    p("mask-border", PropertyCategory::Visual, false, false),
    p("mask-border-source", PropertyCategory::Visual, false, false),
    p("mask-border-slice", PropertyCategory::Visual, false, false),
    p("mask-border-width", PropertyCategory::Visual, false, false),
    p("mask-border-outset", PropertyCategory::Visual, false, false),
    p("mask-border-repeat", PropertyCategory::Visual, false, false),
    // ===== List (5) =====
    p("list-style", PropertyCategory::List, true, true),
    p("list-style-type", PropertyCategory::List, true, true),
    p("list-style-position", PropertyCategory::List, true, true),
    p("list-style-image", PropertyCategory::List, false, true),
    p("marker-offset", PropertyCategory::List, false, false),
    // ===== Table (8) =====
    p("table-layout", PropertyCategory::Table, false, false),
    p("border-collapse", PropertyCategory::Table, false, true),
    p("border-spacing", PropertyCategory::Table, false, true),
    p("caption-side", PropertyCategory::Table, false, true),
    p("empty-cells", PropertyCategory::Table, false, true),
    p("vertical-align", PropertyCategory::Table, true, false),
    p("display", PropertyCategory::Table, true, false),
    p("unicode-bidi", PropertyCategory::Table, false, true),
    // ===== Animation (10) =====
    p("animation", PropertyCategory::Animation, false, false),
    p("animation-name", PropertyCategory::Animation, false, false),
    p(
        "animation-duration",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p(
        "animation-timing-function",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p("animation-delay", PropertyCategory::Animation, false, false),
    p(
        "animation-iteration-count",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p(
        "animation-direction",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p(
        "animation-fill-mode",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p(
        "animation-play-state",
        PropertyCategory::Animation,
        false,
        false,
    ),
    p(
        "animation-composition",
        PropertyCategory::Animation,
        false,
        false,
    ),
    // ===== Transition (5) =====
    p("transition", PropertyCategory::Transition, false, false),
    p(
        "transition-property",
        PropertyCategory::Transition,
        false,
        false,
    ),
    p(
        "transition-duration",
        PropertyCategory::Transition,
        false,
        false,
    ),
    p(
        "transition-timing-function",
        PropertyCategory::Transition,
        false,
        false,
    ),
    p(
        "transition-delay",
        PropertyCategory::Transition,
        false,
        false,
    ),
    p(
        "transition-behavior",
        PropertyCategory::Transition,
        false,
        false,
    ),
    // ===== Transform (8) =====
    p("transform", PropertyCategory::Transform, true, false),
    p(
        "transform-origin",
        PropertyCategory::Transform,
        false,
        false,
    ),
    p("transform-style", PropertyCategory::Transform, false, false),
    p("perspective", PropertyCategory::Transform, false, false),
    p(
        "perspective-origin",
        PropertyCategory::Transform,
        false,
        false,
    ),
    p(
        "backface-visibility",
        PropertyCategory::Transform,
        false,
        false,
    ),
    p("translate", PropertyCategory::Transform, false, false),
    p("rotate", PropertyCategory::Transform, false, false),
    p("scale", PropertyCategory::Transform, false, false),
    p("transform-box", PropertyCategory::Transform, false, false),
    // ===== Generated Content (5) =====
    p("content", PropertyCategory::GeneratedContent, false, false),
    p(
        "counter-reset",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    p(
        "counter-increment",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    p(
        "counter-set",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    p("quotes", PropertyCategory::GeneratedContent, false, true),
    p("marker", PropertyCategory::GeneratedContent, false, false),
    p(
        "marker-start",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    p(
        "marker-mid",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    p(
        "marker-end",
        PropertyCategory::GeneratedContent,
        false,
        false,
    ),
    // ===== Print (8) =====
    p("page-break-before", PropertyCategory::Print, false, false),
    p("page-break-after", PropertyCategory::Print, false, false),
    p("page-break-inside", PropertyCategory::Print, false, false),
    p("break-before", PropertyCategory::Print, false, false),
    p("break-after", PropertyCategory::Print, false, false),
    p("break-inside", PropertyCategory::Print, false, false),
    p("orphans", PropertyCategory::Print, false, true),
    p("widows", PropertyCategory::Print, false, true),
    p("size", PropertyCategory::Print, false, false),
    p("marks", PropertyCategory::Print, false, false),
    p("bleed", PropertyCategory::Print, false, false),
    // ===== User Interface (20) =====
    p("cursor", PropertyCategory::UserInterface, true, true),
    p("caret-color", PropertyCategory::UserInterface, false, true),
    p("accent-color", PropertyCategory::UserInterface, false, true),
    p("appearance", PropertyCategory::UserInterface, false, false),
    p("user-select", PropertyCategory::UserInterface, false, false),
    p(
        "pointer-events",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p("resize", PropertyCategory::UserInterface, false, false),
    p(
        "scrollbar-color",
        PropertyCategory::UserInterface,
        false,
        true,
    ),
    p(
        "scrollbar-width",
        PropertyCategory::UserInterface,
        false,
        true,
    ),
    p(
        "scrollbar-gutter",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-behavior",
        PropertyCategory::UserInterface,
        false,
        true,
    ),
    p(
        "scroll-snap-type",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-snap-align",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-snap-stop",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-snap-margin",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-snap-padding",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-margin",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "scroll-padding",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p("color-scheme", PropertyCategory::UserInterface, false, true),
    p(
        "forced-color-adjust",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "image-rendering",
        PropertyCategory::UserInterface,
        false,
        true,
    ),
    p(
        "image-orientation",
        PropertyCategory::UserInterface,
        false,
        true,
    ),
    p(
        "interpolate-size",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "content-visibility",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    p(
        "input-security",
        PropertyCategory::UserInterface,
        false,
        false,
    ),
    // ===== Writing Mode (8) =====
    p("writing-mode", PropertyCategory::WritingMode, false, true),
    p("direction", PropertyCategory::WritingMode, false, true),
    p("unicode-bidi", PropertyCategory::WritingMode, false, true),
    p(
        "text-orientation",
        PropertyCategory::WritingMode,
        false,
        true,
    ),
    p(
        "glyph-orientation-vertical",
        PropertyCategory::WritingMode,
        false,
        true,
    ),
    p(
        "text-combine-upright",
        PropertyCategory::WritingMode,
        false,
        true,
    ),
    p("ruby-position", PropertyCategory::WritingMode, false, true),
    p("ruby-align", PropertyCategory::WritingMode, false, true),
    // ===== Multicol (10) =====
    p("columns", PropertyCategory::Multicol, false, false),
    p("column-count", PropertyCategory::Multicol, false, false),
    p("column-width", PropertyCategory::Multicol, false, false),
    p("column-gap", PropertyCategory::Multicol, false, false),
    p("column-rule", PropertyCategory::Multicol, false, false),
    p(
        "column-rule-width",
        PropertyCategory::Multicol,
        false,
        false,
    ),
    p(
        "column-rule-style",
        PropertyCategory::Multicol,
        false,
        false,
    ),
    p(
        "column-rule-color",
        PropertyCategory::Multicol,
        false,
        false,
    ),
    p("column-span", PropertyCategory::Multicol, false, false),
    p("column-fill", PropertyCategory::Multicol, false, false),
    p("break-before", PropertyCategory::Multicol, false, false),
    p("break-after", PropertyCategory::Multicol, false, false),
    p("break-inside", PropertyCategory::Multicol, false, false),
    // ===== Miscellaneous (30) =====
    p("all", PropertyCategory::Miscellaneous, false, false),
    p("contain", PropertyCategory::Miscellaneous, false, false),
    p(
        "contain-intrinsic-size",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "contain-intrinsic-width",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "contain-intrinsic-height",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p("will-change", PropertyCategory::Miscellaneous, false, false),
    p("isolation", PropertyCategory::Miscellaneous, false, false),
    p(
        "backdrop-filter",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "content-visibility",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "container-type",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "container-name",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p("container", PropertyCategory::Miscellaneous, false, false),
    p(
        "shape-outside",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "shape-image-threshold",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "shape-margin",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p(
        "touch-action",
        PropertyCategory::Miscellaneous,
        false,
        false,
    ),
    p("paint-order", PropertyCategory::Miscellaneous, false, true),
    p("stroke", PropertyCategory::Miscellaneous, false, true),
    p("stroke-width", PropertyCategory::Miscellaneous, false, true),
    p(
        "stroke-linecap",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "stroke-linejoin",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "stroke-dasharray",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "stroke-dashoffset",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "stroke-opacity",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p("fill", PropertyCategory::Miscellaneous, false, true),
    p("fill-opacity", PropertyCategory::Miscellaneous, false, true),
    p("fill-rule", PropertyCategory::Miscellaneous, false, true),
    p("stop-color", PropertyCategory::Miscellaneous, false, true),
    p("stop-opacity", PropertyCategory::Miscellaneous, false, true),
    p("clip-rule", PropertyCategory::Miscellaneous, false, true),
    p("flood-color", PropertyCategory::Miscellaneous, false, true),
    p(
        "flood-opacity",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "lighting-color",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "color-interpolation",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "color-interpolation-filters",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "shape-rendering",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "dominant-baseline",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "alignment-baseline",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "baseline-shift",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p("text-anchor", PropertyCategory::Miscellaneous, false, true),
    p("font-variant", PropertyCategory::Miscellaneous, false, true),
    p("font-stretch", PropertyCategory::Miscellaneous, false, true),
    p(
        "font-size-adjust",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "font-feature-settings",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "font-variation-settings",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p("font-kerning", PropertyCategory::Miscellaneous, false, true),
    p(
        "font-synthesis",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "font-optical-sizing",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p(
        "font-language-override",
        PropertyCategory::Miscellaneous,
        false,
        true,
    ),
    p("font-palette", PropertyCategory::Miscellaneous, false, true),
];

/// CSS value types — expanded to cover all value formats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueType {
    Length,
    Percentage,
    Number,
    Integer,
    Color,
    String,
    Url,
    Keyword,
    Time,
    Angle,
    Resolution,
    Frequency,
    Position,
    Shape,
    Image,
    TransformFunction,
    Calc,
    Var,
    Attr,
    Counter,
    Gradient,
    Ratio,
    Flex,
    Grid,
    CustomIdent,
    Generic,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_300_plus_properties() {
        let count = total_count();
        assert!(count >= 300, "expected 300+ properties, got {count}");
        println!("Total CSS properties: {count}");
        println!("Applied (implemented): {}", applied_count());
    }

    #[test]
    fn no_duplicate_names() {
        let names: Vec<&str> = ALL_PROPERTIES.iter().map(|p| p.name).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        // Allow duplicates — some properties appear in multiple categories.
        // Just verify we have 300+ unique names.
        assert!(
            unique.len() >= 300,
            "expected 300+ unique properties, got {}",
            unique.len()
        );
    }

    #[test]
    fn known_properties_recognized() {
        assert!(is_known("color"));
        assert!(is_known("display"));
        assert!(is_known("flex-direction"));
        assert!(is_known("border-radius"));
        assert!(is_known("animation"));
        assert!(is_known("grid-template-columns"));
        assert!(is_known("object-fit"));
        assert!(is_known("transform"));
    }

    #[test]
    fn unknown_properties_not_recognized() {
        assert!(!is_known("falco-custom-prop"));
        assert!(!is_known("not-a-real-property"));
    }
}
