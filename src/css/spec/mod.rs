//! CSS2 — spec-compliant CSS engine (cascade, selectors, advanced features).
//!
//! This module complements the existing `css` module with full implementations
//! of:
//!
//! * **Cascade specificity** — (a, b, c) tuple ordering per Selectors L4.
//! * **Selector matching** — full Level 4 selector engine with `:has()`,
//!   `:is()`, `:where()`, `:nth-child(an+b of S)`, all combinators.
//! * **Cascade Layers** (`@layer`) — tiered priority above origin.
//! * **Container Queries** (`@container`) — responsive queries.
//! * **@font-face** — custom font declarations with lookup by family/weight.
//! * **@keyframes + animations** — cubic-bezier easing, interpolation.
//! * **Transitions** — smooth value interpolation.
//! * **clip-path** — polygon, circle, ellipse, inset, path.
//! * **filter / backdrop-filter** — blur, brightness, contrast, etc.
//! * **Containment** — `contain: layout/paint/size/style`.
//! * **CSS Counters** — counter-reset, counter-increment, counter().
//! * **Writing modes** — horizontal-tb, vertical-rl/lr, sideways-rl/lr.
//! * **Logical properties** — margin-inline-start, etc. (mapped via writing mode).

pub mod advanced;
pub mod cascade;
pub mod selectors;

pub use advanced::{
    evaluate_container_query, interpolate, Animation, AnimationDirection, AnimationFillMode,
    AnimationPlayState, CascadeLayers, ClipPath, ComparisonOp, ContainerCondition,
    ContainerFeature, ContainerQuery, ContainerUnit, Containment, CounterState, FilterFunction,
    FontDisplay, FontFace, FontFaceRegistry, FontSource, Keyframe, Keyframes, StepPosition,
    TimingFunction, Transition, WritingMode,
};
pub use cascade::{
    cascade_compare, selector_specificity, CascadeOrigin, CascadedDeclaration, Specificity,
};
pub use selectors::{
    matches, AttrOp, Combinator, ComplexSelector, CompoundSelector, PseudoClass, SelectorList,
    SimpleSelector,
};

/// Logical property → physical property mapping based on writing mode.
///
/// `margin-inline-start` becomes `margin-left` in horizontal-tb mode, but
/// `margin-top` in vertical-rl mode, etc.
pub fn resolve_logical_property(name: &str, mode: WritingMode) -> &str {
    match (name, mode.is_vertical()) {
        ("margin-inline-start", false) | ("margin-block-start", true) => "margin-left",
        ("margin-inline-end", false) | ("margin-block-end", true) => "margin-right",
        ("margin-block-start", false) | ("margin-inline-start", true) => "margin-top",
        ("margin-block-end", false) | ("margin-inline-end", true) => "margin-bottom",
        ("padding-inline-start", false) | ("padding-block-start", true) => "padding-left",
        ("padding-inline-end", false) | ("padding-block-end", true) => "padding-right",
        ("padding-block-start", false) | ("padding-inline-start", true) => "padding-top",
        ("padding-block-end", false) | ("padding-inline-end", true) => "padding-bottom",
        ("inset-inline-start", false) | ("inset-block-start", true) => "left",
        ("inset-inline-end", false) | ("inset-block-end", true) => "right",
        ("inset-block-start", false) | ("inset-inline-start", true) => "top",
        ("inset-block-end", false) | ("inset-inline-end", true) => "bottom",
        ("inline-size", false) => "width",
        ("inline-size", true) => "height",
        ("block-size", false) => "height",
        ("block-size", true) => "width",
        ("border-inline-start-width", false) => "border-left-width",
        ("border-inline-end-width", false) => "border-right-width",
        ("border-block-start-width", false) => "border-top-width",
        ("border-block-end-width", false) => "border-bottom-width",
        _ => name, // unknown — pass through
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_to_physical_horizontal() {
        assert_eq!(
            resolve_logical_property("margin-inline-start", WritingMode::HorizontalTb),
            "margin-left"
        );
        assert_eq!(
            resolve_logical_property("margin-block-start", WritingMode::HorizontalTb),
            "margin-top"
        );
        assert_eq!(
            resolve_logical_property("inline-size", WritingMode::HorizontalTb),
            "width"
        );
    }

    #[test]
    fn logical_to_physical_vertical() {
        assert_eq!(
            resolve_logical_property("margin-block-start", WritingMode::VerticalRl),
            "margin-left"
        );
        assert_eq!(
            resolve_logical_property("margin-inline-start", WritingMode::VerticalRl),
            "margin-top"
        );
        assert_eq!(
            resolve_logical_property("inline-size", WritingMode::VerticalRl),
            "height"
        );
    }
}
