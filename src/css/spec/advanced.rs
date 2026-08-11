//! CSS at-rules and advanced features.
//!
//! Implements:
//! * **Cascade Layers** (`@layer`) — priority tiers above origin/importance.
//! * **Container Queries** (`@container`) — responsive queries based on
//!   the nearest ancestor with `container-type`.
//! * **@font-face** — custom font declarations with substitution.
//! * **@keyframes** + animations — interpolation of properties over time.
//! * **Transitions** — smooth interpolation between property values.
//! * **clip-path** — geometric clipping (polygon, circle, ellipse, path).
//! * **filter** — pixel filters (blur, brightness, contrast, drop-shadow, etc.).
//! * **backdrop-filter** — filters applied to the background behind an element.

use std::collections::HashMap;

// ===================== Cascade Layers =====================

/// A cascade layer registry. Tracks named layers in declaration order.
#[derive(Debug, Clone, Default)]
pub struct CascadeLayers {
    /// Layer names in declaration order. Later = higher priority.
    pub layers: Vec<String>,
}

impl CascadeLayers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a layer (or layers — `@layer a, b, c;` declares three).
    /// Does nothing if the layer already exists.
    pub fn declare(&mut self, name: &str) {
        if !self.layers.iter().any(|n| n == name) {
            self.layers.push(name.to_string());
        }
    }

    /// Get the priority index of a layer. Higher = wins.
    /// Unlayered declarations implicitly have index = layers.len().
    pub fn priority(&self, name: &str) -> usize {
        self.layers
            .iter()
            .position(|n| n == name)
            .unwrap_or(self.layers.len())
    }
}

// ===================== Container Queries =====================

/// A container query. `@container (min-width: 600px) { ... }`.
#[derive(Debug, Clone)]
pub struct ContainerQuery {
    /// Optional container name. If None, matches the nearest ancestor with
    /// `container-type` set.
    pub name: Option<String>,
    pub conditions: Vec<ContainerCondition>,
}

#[derive(Debug, Clone)]
pub struct ContainerCondition {
    pub feature: ContainerFeature,
    pub operator: ComparisonOp,
    pub value: f32,
    pub unit: ContainerUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFeature {
    Width,
    Height,
    InlineSize,
    BlockSize,
    AspectRatio,
    Orientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Min, // >=
    Max, // <=
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerUnit {
    Px,
    Em,
    Rem,
    Percent,
}

/// Evaluate a container query against a container's dimensions.
pub fn evaluate_container_query(
    query: &ContainerQuery,
    container_width: f32,
    container_height: f32,
) -> bool {
    query.conditions.iter().all(|cond| {
        let actual = match cond.feature {
            ContainerFeature::Width | ContainerFeature::InlineSize => container_width,
            ContainerFeature::Height | ContainerFeature::BlockSize => container_height,
            ContainerFeature::AspectRatio => container_width / container_height.max(1.0),
            ContainerFeature::Orientation => {
                if container_width >= container_height {
                    1.0
                } else {
                    0.0
                }
            }
        };
        let target = match cond.unit {
            ContainerUnit::Px => cond.value,
            ContainerUnit::Em | ContainerUnit::Rem => cond.value * 16.0,
            ContainerUnit::Percent => cond.value / 100.0 * container_width,
        };
        match cond.operator {
            ComparisonOp::Min => actual >= target,
            ComparisonOp::Max => actual <= target,
            ComparisonOp::Equal => (actual - target).abs() < 0.01,
        }
    })
}

// ===================== @font-face =====================

/// A `@font-face` declaration.
#[derive(Debug, Clone)]
pub struct FontFace {
    pub family: String,
    pub src: Vec<FontSource>,
    pub style: Option<String>,
    pub weight: Option<u32>,
    pub stretch: Option<String>,
    pub display: FontDisplay,
    pub unicode_range: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub enum FontSource {
    Url { url: String, format: Option<String> },
    Local(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontDisplay {
    Auto,
    Block,
    Swap,
    Fallback,
    Optional,
}

/// A registry of @font-face declarations.
#[derive(Debug, Clone, Default)]
pub struct FontFaceRegistry {
    pub fonts: Vec<FontFace>,
}

impl FontFaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, font: FontFace) {
        self.fonts.push(font);
    }

    /// Find a matching @font-face for the requested family/style/weight.
    pub fn find(
        &self,
        family: &str,
        style: Option<&str>,
        weight: Option<u32>,
    ) -> Option<&FontFace> {
        self.fonts.iter().find(|f| {
            f.family.eq_ignore_ascii_case(family)
                && f.style.as_deref() == style
                && f.weight == weight
        })
    }
}

// ===================== @keyframes + Animations =====================

/// A `@keyframes` rule.
#[derive(Debug, Clone)]
pub struct Keyframes {
    pub name: String,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone)]
pub struct Keyframe {
    /// 0.0 to 1.0. `from` = 0.0, `to` = 1.0.
    pub offset: f32,
    /// Property declarations at this offset.
    pub declarations: Vec<(String, String)>,
}

/// An `animation` shorthand expansion.
#[derive(Debug, Clone)]
pub struct Animation {
    pub name: String,
    pub duration: f32, // seconds
    pub timing_function: TimingFunction,
    pub delay: f32, // seconds
    pub iteration_count: f32,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPlayState {
    Running,
    Paused,
}

#[derive(Debug, Clone)]
pub enum TimingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, StepPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPosition {
    Start,
    End,
    JumpNone,
    JumpBoth,
}

impl TimingFunction {
    /// Evaluate the timing function at time `t` (0.0 to 1.0).
    pub fn evaluate(&self, t: f32) -> f32 {
        match self {
            TimingFunction::Linear => t,
            TimingFunction::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
            TimingFunction::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
            TimingFunction::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
            TimingFunction::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
            TimingFunction::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, t),
            TimingFunction::Steps(n, pos) => {
                let step = 1.0 / *n as f32;
                let count = (t / step).floor() as i32;
                let count = match pos {
                    StepPosition::Start | StepPosition::JumpNone => count + 1,
                    StepPosition::End | StepPosition::JumpBoth => count,
                };
                (count as f32).min(*n as f32) * step
            }
        }
    }
}

/// Evaluate a cubic-bezier easing function.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    // Newton-Raphson to find the bezier parameter s such that X(s) = t.
    let mut s = t;
    for _ in 0..8 {
        let x = bezier_component(s, x1, x2);
        let dx = bezier_derivative(s, x1, x2);
        if dx.abs() < 1e-6 {
            break;
        }
        s -= (x - t) / dx;
        s = s.max(0.0).min(1.0);
    }
    bezier_component(s, y1, y2)
}

fn bezier_component(t: f32, p1: f32, p2: f32) -> f32 {
    // B(t) = 3(1-t)^2 t p1 + 3(1-t) t^2 p2 + t^3
    let one_t = 1.0 - t;
    3.0 * one_t * one_t * t * p1 + 3.0 * one_t * t * t * p2 + t * t * t
}

fn bezier_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    let one_t = 1.0 - t;
    3.0 * one_t * one_t * p1 + 6.0 * one_t * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

/// Compute the interpolated value of a property at a given animation
/// progress (0.0 to 1.0).
pub fn interpolate(_property: &str, from: &str, to: &str, t: f32) -> String {
    // Try numeric interpolation first.
    if let (Some(a), Some(b)) = (parse_number_with_unit(from), parse_number_with_unit(to)) {
        let v = a.0 + (b.0 - a.0) * t;
        return format!(
            "{}{}",
            v,
            if a.1.is_empty() {
                b.1.clone()
            } else {
                a.1.clone()
            }
        );
    }
    // Color interpolation.
    if let (Some(c1), Some(c2)) = (parse_color(from), parse_color(to)) {
        let r = (c1.0 as f32 + (c2.0 as f32 - c1.0 as f32) * t).round() as u8;
        let g = (c1.1 as f32 + (c2.1 as f32 - c1.1 as f32) * t).round() as u8;
        let b = (c1.2 as f32 + (c2.2 as f32 - c1.2 as f32) * t).round() as u8;
        let a = (c1.3 as f32 + (c2.3 as f32 - c1.3 as f32) * t).round() as u8;
        return format!("rgba({}, {}, {}, {})", r, g, b, a as f32 / 255.0);
    }
    // For non-interpolable types, snap at t=0.5.
    if t < 0.5 {
        from.to_string()
    } else {
        to.to_string()
    }
}

fn parse_number_with_unit(s: &str) -> Option<(f32, String)> {
    let s = s.trim();
    let mut num_end = 0;
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
            num_end = i + 1;
        } else {
            break;
        }
    }
    if num_end == 0 {
        return None;
    }
    let num: f32 = s[..num_end].parse().ok()?;
    let unit = s[num_end..].trim().to_string();
    Some((num, unit))
}

fn parse_color(s: &str) -> Option<(u8, u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r, g, b, 255));
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            return Some((r, g, b, a));
        }
    }
    if let Some(rest) = s.strip_prefix("rgb(") {
        let parts: Vec<&str> = rest.trim_end_matches(')').split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some((r, g, b, 255));
        }
    }
    if let Some(rest) = s.strip_prefix("rgba(") {
        let parts: Vec<&str> = rest.trim_end_matches(')').split(',').collect();
        if parts.len() == 4 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            let a_f: f32 = parts[3].trim().parse().ok()?;
            return Some((r, g, b, (a_f * 255.0) as u8));
        }
    }
    None
}

// ===================== Transitions =====================

/// A `transition` shorthand expansion.
#[derive(Debug, Clone)]
pub struct Transition {
    pub property: String,
    pub duration: f32,
    pub delay: f32,
    pub timing_function: TimingFunction,
}

// ===================== clip-path =====================

#[derive(Debug, Clone)]
pub enum ClipPath {
    None,
    /// `clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%)`
    Polygon(Vec<(f32, f32)>),
    /// `clip-path: circle(50% at 50% 50%)`
    Circle {
        radius: f32,
        cx: f32,
        cy: f32,
    },
    /// `clip-path: ellipse(50% 25% at 50% 50%)`
    Ellipse {
        rx: f32,
        ry: f32,
        cx: f32,
        cy: f32,
    },
    /// `clip-path: inset(10px 20px 30px 40px round 5px)`
    Inset {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
        round: f32,
    },
    /// `clip-path: path("M ... Z")`
    Path(String),
    /// Reference to an SVG `<clipPath>` element by URL.
    Url(String),
}

impl std::fmt::Display for ClipPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipPath::None => write!(f, "none"),
            ClipPath::Polygon(points) => {
                write!(f, "polygon(")?;
                for (i, (x, y)) in points.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}px {}px", x, y)?;
                }
                write!(f, ")")
            }
            ClipPath::Circle { radius, cx, cy } => {
                write!(f, "circle({}px at {}px {}px)", radius, cx, cy)
            }
            ClipPath::Ellipse { rx, ry, cx, cy } => {
                write!(f, "ellipse({}px {}px at {}px {}px)", rx, ry, cx, cy)
            }
            ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                round,
            } => write!(
                f,
                "inset({}px {}px {}px {}px round {}px)",
                top, right, bottom, left, round
            ),
            ClipPath::Path(p) => write!(f, "path(\"{}\")", p),
            ClipPath::Url(u) => write!(f, "url({})", u),
        }
    }
}

// ===================== filter / backdrop-filter =====================

#[derive(Debug, Clone)]
pub enum FilterFunction {
    /// `blur(2px)` — Gaussian blur.
    Blur(f32),
    /// `brightness(1.5)` — 1.0 = normal.
    Brightness(f32),
    /// `contrast(1.5)`.
    Contrast(f32),
    /// `drop-shadow(2px 4px 4px rgba(0,0,0,0.5))`.
    DropShadow {
        dx: f32,
        dy: f32,
        blur: f32,
        color: (u8, u8, u8, u8),
    },
    /// `grayscale(0.5)` — 0 = none, 1 = full.
    Grayscale(f32),
    /// `hue-rotate(90deg)`.
    HueRotate(f32),
    /// `invert(0.5)`.
    Invert(f32),
    /// `opacity(0.5)`.
    Opacity(f32),
    /// `saturate(2.0)`.
    Saturate(f32),
    /// `sepia(0.5)`.
    Sepia(f32),
}

impl std::fmt::Display for FilterFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterFunction::Blur(v) => write!(f, "blur({}px)", v),
            FilterFunction::Brightness(v) => write!(f, "brightness({})", v),
            FilterFunction::Contrast(v) => write!(f, "contrast({})", v),
            FilterFunction::DropShadow {
                dx,
                dy,
                blur,
                color,
            } => write!(
                f,
                "drop-shadow({}px {}px {}px rgba({},{},{},{}))",
                dx, dy, blur, color.0, color.1, color.2, color.3
            ),
            FilterFunction::Grayscale(v) => write!(f, "grayscale({})", v),
            FilterFunction::HueRotate(v) => write!(f, "hue-rotate({}deg)", v),
            FilterFunction::Invert(v) => write!(f, "invert({})", v),
            FilterFunction::Opacity(v) => write!(f, "opacity({})", v),
            FilterFunction::Saturate(v) => write!(f, "saturate({})", v),
            FilterFunction::Sepia(v) => write!(f, "sepia({})", v),
        }
    }
}

// ===================== Containment =====================

/// `contain: layout paint size style`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Containment {
    pub layout: bool,
    pub paint: bool,
    pub size: bool,
    pub style: bool,
    pub inline_size: bool,
    pub block_size: bool,
}

impl Containment {
    /// Parse a `contain` shorthand value.
    pub fn parse(s: &str) -> Self {
        let mut c = Self::default();
        for token in s.split_whitespace() {
            match token {
                "layout" => c.layout = true,
                "paint" => c.paint = true,
                "size" => {
                    c.size = true;
                    c.inline_size = true;
                    c.block_size = true;
                }
                "inline-size" => c.inline_size = true,
                "block-size" => c.block_size = true,
                "style" => c.style = true,
                "strict" => {
                    c.layout = true;
                    c.paint = true;
                    c.size = true;
                    c.inline_size = true;
                    c.block_size = true;
                }
                "content" => {
                    c.layout = true;
                    c.paint = true;
                    c.style = true;
                }
                "none" => {}
                _ => {}
            }
        }
        c
    }
}

// ===================== CSS Counters =====================

#[derive(Debug, Clone, Default)]
pub struct CounterState {
    /// Map from counter name → value.
    pub counters: HashMap<String, i32>,
}

impl CounterState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment(&mut self, name: &str, by: i32) {
        *self.counters.entry(name.to_string()).or_insert(0) += by;
    }

    pub fn set(&mut self, name: &str, value: i32) {
        self.counters.insert(name.to_string(), value);
    }

    pub fn reset(&mut self, name: &str, to: i32) {
        self.counters.insert(name.to_string(), to);
    }

    pub fn get(&self, name: &str) -> i32 {
        *self.counters.get(name).unwrap_or(&0)
    }
}

// ===================== Writing Modes =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "horizontal-tb" => Some(Self::HorizontalTb),
            "vertical-rl" => Some(Self::VerticalRl),
            "vertical-lr" => Some(Self::VerticalLr),
            "sideways-rl" => Some(Self::SidewaysRl),
            "sideways-lr" => Some(Self::SidewaysLr),
            _ => None,
        }
    }

    pub fn is_vertical(&self) -> bool {
        matches!(
            self,
            Self::VerticalRl | Self::VerticalLr | Self::SidewaysRl | Self::SidewaysLr
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_layers_priority() {
        let mut layers = CascadeLayers::new();
        layers.declare("base");
        layers.declare("components");
        layers.declare("utilities");
        assert_eq!(layers.priority("base"), 0);
        assert_eq!(layers.priority("components"), 1);
        assert_eq!(layers.priority("utilities"), 2);
        // Unlayered = highest priority (3, the count of layers).
        assert_eq!(layers.priority("nonexistent"), 3);
    }

    #[test]
    fn container_query_evaluation() {
        let query = ContainerQuery {
            name: None,
            conditions: vec![ContainerCondition {
                feature: ContainerFeature::Width,
                operator: ComparisonOp::Min,
                value: 600.0,
                unit: ContainerUnit::Px,
            }],
        };
        assert!(evaluate_container_query(&query, 800.0, 600.0));
        assert!(!evaluate_container_query(&query, 400.0, 600.0));
    }

    #[test]
    fn font_face_lookup() {
        let mut reg = FontFaceRegistry::new();
        reg.add(FontFace {
            family: "MyFont".into(),
            src: vec![FontSource::Local("MyFont".into())],
            style: Some("normal".into()),
            weight: Some(400),
            stretch: None,
            display: FontDisplay::Swap,
            unicode_range: None,
        });
        let f = reg.find("myfont", Some("normal"), Some(400));
        assert!(f.is_some());
    }

    #[test]
    fn cubic_bezier_evaluate() {
        let tf = TimingFunction::Ease;
        assert!((tf.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((tf.evaluate(1.0) - 1.0).abs() < 0.01);
        // At t=0.5, ease should be somewhere between 0 and 1.
        let v = tf.evaluate(0.5);
        assert!(v > 0.0 && v < 1.0);
    }

    #[test]
    fn interpolate_numbers() {
        let v = interpolate("width", "10px", "20px", 0.5);
        assert!(v.contains("15"), "got: {}", v);
    }

    #[test]
    fn interpolate_colors() {
        let v = interpolate("color", "#000000", "#ffffff", 0.5);
        // Should be approximately (127 or 128, depending on rounding).
        assert!(v.contains("127") || v.contains("128"), "got: {}", v);
    }

    #[test]
    fn clip_path_display() {
        let cp = ClipPath::Circle {
            radius: 50.0,
            cx: 50.0,
            cy: 50.0,
        };
        let s = format!("{}", cp);
        assert!(s.contains("circle"));
        assert!(s.contains("50px"));
    }

    #[test]
    fn filter_display() {
        let f = FilterFunction::Blur(2.0);
        assert_eq!(format!("{}", f), "blur(2px)");
    }

    #[test]
    fn containment_parse() {
        let c = Containment::parse("layout paint");
        assert!(c.layout);
        assert!(c.paint);
        assert!(!c.size);

        let strict = Containment::parse("strict");
        assert!(strict.layout && strict.paint && strict.size);
    }

    #[test]
    fn counter_state_operations() {
        let mut s = CounterState::new();
        s.reset("section", 0);
        s.increment("section", 1);
        s.increment("section", 1);
        assert_eq!(s.get("section"), 2);
    }

    #[test]
    fn writing_mode_parsing() {
        assert_eq!(
            WritingMode::parse("horizontal-tb"),
            Some(WritingMode::HorizontalTb)
        );
        assert_eq!(
            WritingMode::parse("vertical-rl"),
            Some(WritingMode::VerticalRl)
        );
        assert_eq!(
            WritingMode::parse("vertical-rl").unwrap().is_vertical(),
            true
        );
        assert_eq!(
            WritingMode::parse("horizontal-tb").unwrap().is_vertical(),
            false
        );
    }

    #[test]
    fn steps_timing() {
        let tf = TimingFunction::Steps(4, StepPosition::End);
        // At t=0.25, we should be at step 1 (= 0.25).
        let v = tf.evaluate(0.25);
        assert!((v - 0.25).abs() < 0.01, "got: {}", v);
    }
}
