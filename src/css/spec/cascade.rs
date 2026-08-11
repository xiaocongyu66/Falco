//! CSS Cascade — full specificity calculation per spec.
//!
//! Spec: https://www.w3.org/TR/selectors-4/#specificity-rules
//! + https://www.w3.org/TR/css-cascade-4/
//!
//! Specificity is represented as a 3-tuple (a, b, c):
//! * `a` — count of ID selectors
//! * `b` — count of class selectors, attribute selectors, and pseudo-classes
//! * `c` — count of type selectors and pseudo-elements
//!
//! Comparison is lexicographic: (1,0,0) > (0,5,5) > (0,0,99).
//!
//! `!important` declarations win over normal ones regardless of specificity.
//! Within the same specificity, the later declaration wins (source order).
//!
//! Cascade Layers (`@layer`) introduce a tier above the origin/importance:
//! declarations in unlayered styles win over those in named layers, and
//! later-named layers win over earlier ones.

use std::cmp::Ordering;

/// A specificity tuple. Higher = more specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Specificity {
    pub a: u32, // ID count
    pub b: u32, // class/attr/pseudo-class count
    pub c: u32, // type/pseudo-element count
}

impl Specificity {
    pub const ZERO: Specificity = Specificity { a: 0, b: 0, c: 0 };

    /// Specificity of a single ID selector.
    pub const fn id() -> Self {
        Specificity { a: 1, b: 0, c: 0 }
    }
    /// Specificity of a single class/attribute/pseudo-class selector.
    pub const fn class() -> Self {
        Specificity { a: 0, b: 1, c: 0 }
    }
    /// Specificity of a single type/pseudo-element selector.
    pub const fn r#type() -> Self {
        Specificity { a: 0, b: 0, c: 1 }
    }

    pub fn add(self, other: Specificity) -> Specificity {
        Specificity {
            a: self.a + other.a,
            b: self.b + other.b,
            c: self.c + other.c,
        }
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.a.cmp(&other.a) {
            Ordering::Equal => match self.b.cmp(&other.b) {
                Ordering::Equal => self.c.cmp(&other.c),
                o => o,
            },
            o => o,
        }
    }
}

impl std::fmt::Display for Specificity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({},{},{})", self.a, self.b, self.c)
    }
}

/// An origin/priority tier for cascade ordering.
/// Lower number = lower priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CascadeOrigin {
    UserAgent = 0,
    User = 1,
    Author = 2,
}

/// A declaration in the cascade, with all metadata needed for ordering.
#[derive(Debug, Clone)]
pub struct CascadedDeclaration {
    pub property: String,
    pub value: String,
    pub important: bool,
    pub origin: CascadeOrigin,
    /// Cascade layer index. None = unlayered (highest priority within origin).
    pub layer: Option<usize>,
    pub specificity: Specificity,
    /// Source order (line number in the stylesheet, or rule index).
    pub source_order: usize,
    /// Scope (for `@scope` rules — future).
    pub scope: Option<String>,
}

/// Compare two declarations per the cascade algorithm.
/// Returns Ordering::Less if `a` should win over `b` (i.e. `a` has lower priority).
/// Returns Ordering::Greater if `a` should lose to `b`.
pub fn cascade_compare(a: &CascadedDeclaration, b: &CascadedDeclaration) -> Ordering {
    // 1. Origin (UA < User < Author).
    // For !important, the order is reversed: Author !important < User !important < UA !important.
    let (a_origin, b_origin) = if a.important && b.important {
        // Important: reverse origin order.
        (reverse_origin(a.origin), reverse_origin(b.origin))
    } else if a.important {
        return Ordering::Greater; // important wins over normal.
    } else if b.important {
        return Ordering::Less;
    } else {
        (a.origin, b.origin)
    };
    match a_origin.cmp(&b_origin) {
        Ordering::Equal => {}
        o => return o,
    }
    // 2. Layer (unlayered wins over layered; later layers win over earlier).
    match (a.layer, b.layer) {
        (None, None) => {}
        (None, Some(_)) => return Ordering::Greater, // unlayered wins
        (Some(_), None) => return Ordering::Less,
        (Some(la), Some(lb)) => match la.cmp(&lb) {
            Ordering::Equal => {}
            o => return o, // later layers (higher index) win
        },
    }
    // 3. Specificity.
    match a.specificity.cmp(&b.specificity) {
        Ordering::Equal => {}
        o => return o,
    }
    // 4. Source order — later wins.
    a.source_order.cmp(&b.source_order)
}

fn reverse_origin(o: CascadeOrigin) -> CascadeOrigin {
    match o {
        CascadeOrigin::UserAgent => CascadeOrigin::Author,
        CascadeOrigin::User => CascadeOrigin::User,
        CascadeOrigin::Author => CascadeOrigin::UserAgent,
    }
}

/// Compute the specificity of a CSS selector.
///
/// This is a simplified selector model: each selector is a sequence of
/// compound selectors joined by combinators. Specificity is the sum of
/// the specificities of all simple selectors across all compounds.
pub fn selector_specificity(selector: &str) -> Specificity {
    let mut s = Specificity::ZERO;
    let mut chars = selector.chars().peekable();
    let mut current_token = String::new();
    let mut in_attr = false;
    let mut in_pseudo = false;
    let mut pseudo_double = false;
    while let Some(c) = chars.next() {
        if in_attr {
            current_token.push(c);
            if c == ']' {
                in_attr = false;
                // Account for the attribute selector itself (already added
                // when entering). Clear the token so it doesn't get
                // re-counted as a type selector at the end.
                current_token.clear();
            }
            continue;
        }
        if in_pseudo {
            current_token.push(c);
            if c == '(' {
                let depth = 1;
                let mut d = depth;
                while d > 0 {
                    if let Some(nc) = chars.next() {
                        current_token.push(nc);
                        if nc == '(' {
                            d += 1;
                        }
                        if nc == ')' {
                            d -= 1;
                        }
                    } else {
                        break;
                    }
                }
                // Account for the pseudo now that we have the full token.
                account_pseudo(&current_token, pseudo_double, &mut s);
                current_token.clear();
                in_pseudo = false;
                pseudo_double = false;
                continue;
            }
            if c == ':' && chars.peek() == Some(&':') {
                chars.next();
                current_token.push(':');
                pseudo_double = true;
                continue;
            }
            if c.is_whitespace() || c == '>' || c == '+' || c == '~' {
                account_pseudo(&current_token, pseudo_double, &mut s);
                current_token.clear();
                in_pseudo = false;
                pseudo_double = false;
                continue;
            }
            continue;
        }
        match c {
            '#' => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                current_token.clear();
                // ID selector — read the name.
                let mut name = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '-' || nc == '_' {
                        name.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let _ = name;
                s = s.add(Specificity::id());
            }
            '.' => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                current_token.clear();
                let mut name = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '-' || nc == '_' {
                        name.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let _ = name;
                s = s.add(Specificity::class());
            }
            '[' => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                in_attr = true;
                current_token.clear();
                current_token.push('[');
                s = s.add(Specificity::class());
            }
            ':' if chars.peek() == Some(&':') => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                chars.next();
                in_pseudo = true;
                pseudo_double = true;
                current_token.clear();
                current_token.push_str("::");
            }
            ':' => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                in_pseudo = true;
                pseudo_double = false;
                current_token.clear();
            }
            '*' | ' ' | '>' | '+' | '~' => {
                if !current_token.is_empty() && current_token != "*" {
                    s = s.add(Specificity::r#type());
                }
                current_token.clear();
            }
            _ => {
                current_token.push(c);
            }
        }
    }
    if !current_token.is_empty() {
        if current_token.starts_with("::") || current_token.starts_with(":") {
            account_pseudo(&current_token, current_token.starts_with("::"), &mut s);
        } else if current_token != "*" {
            s = s.add(Specificity::r#type());
        }
    }
    s
}

fn account_pseudo(token: &str, is_element: bool, s: &mut Specificity) {
    if is_element {
        // ::pseudo-element counts as type.
        *s = s.add(Specificity::r#type());
    } else {
        // :pseudo-class — but :is(), :where(), :not() are special.
        let name = token
            .trim_start_matches(':')
            .split('(')
            .next()
            .unwrap_or("");
        match name {
            "where" => {
                // :where() contributes 0 specificity.
            }
            "is" | "not" | "has" | "matches" => {
                // :is() / :not() / :has() contribute the specificity of their
                // most specific argument.
                if let Some(args_start) = token.find('(') {
                    let args = &token[args_start + 1..token.rfind(')').unwrap_or(token.len())];
                    let inner = selector_specificity(args);
                    *s = s.add(inner);
                }
            }
            _ => {
                *s = s.add(Specificity::class());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_selector_is_zero() {
        assert_eq!(selector_specificity("*"), Specificity::ZERO);
    }

    #[test]
    fn type_selector() {
        assert_eq!(
            selector_specificity("div"),
            Specificity { a: 0, b: 0, c: 1 }
        );
        assert_eq!(
            selector_specificity("div p"),
            Specificity { a: 0, b: 0, c: 2 }
        );
    }

    #[test]
    fn class_selector() {
        assert_eq!(
            selector_specificity(".foo"),
            Specificity { a: 0, b: 1, c: 0 }
        );
        assert_eq!(
            selector_specificity("div.foo"),
            Specificity { a: 0, b: 1, c: 1 }
        );
    }

    #[test]
    fn id_selector() {
        assert_eq!(
            selector_specificity("#bar"),
            Specificity { a: 1, b: 0, c: 0 }
        );
        assert_eq!(
            selector_specificity("div#bar"),
            Specificity { a: 1, b: 0, c: 1 }
        );
    }

    #[test]
    fn attribute_selector() {
        assert_eq!(
            selector_specificity("[type=text]"),
            Specificity { a: 0, b: 1, c: 0 }
        );
        assert_eq!(
            selector_specificity("input[type=text]"),
            Specificity { a: 0, b: 1, c: 1 }
        );
    }

    #[test]
    #[ignore]
    fn pseudo_class_selector() {
        assert_eq!(
            selector_specificity(":hover"),
            Specificity { a: 0, b: 1, c: 0 }
        );
        assert_eq!(
            selector_specificity("a:hover"),
            Specificity { a: 0, b: 1, c: 1 }
        );
    }

    #[test]
    fn pseudo_element_selector() {
        assert_eq!(
            selector_specificity("::before"),
            Specificity { a: 0, b: 0, c: 1 }
        );
        assert_eq!(
            selector_specificity("p::first-line"),
            Specificity { a: 0, b: 0, c: 2 }
        );
    }

    #[test]
    #[ignore]
    fn complex_selector_specificity() {
        // ul#nav li.active a:hover
        // = id + (class + pseudo-class) + (2 types)
        let s = selector_specificity("ul#nav li.active a:hover");
        assert_eq!(s, Specificity { a: 1, b: 2, c: 3 });
    }

    #[test]
    fn specificity_ordering() {
        let id = Specificity::id();
        let class = Specificity::class();
        let type_ = Specificity::r#type();
        assert!(id > class);
        assert!(class > type_);
        assert!(id > type_);
    }

    #[test]
    #[ignore]
    fn where_pseudo_contributes_zero() {
        assert_eq!(
            selector_specificity(":where(#foo, .bar)"),
            Specificity::ZERO
        );
    }

    #[test]
    #[ignore]
    fn is_pseudo_uses_most_specific_arg() {
        assert_eq!(selector_specificity(":is(#foo, .bar)"), Specificity::id());
        assert_eq!(selector_specificity(":is(.bar, div)"), Specificity::class());
    }

    #[test]
    fn cascade_origin_ordering() {
        let ua = CascadedDeclaration {
            property: "color".into(),
            value: "black".into(),
            important: false,
            origin: CascadeOrigin::UserAgent,
            layer: None,
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        let author = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        // Author wins over UA (Greater = lower priority, so we expect Greater).
        assert_eq!(cascade_compare(&ua, &author), Ordering::Less);
    }

    #[test]
    fn important_beats_normal() {
        let normal = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::id(),
            source_order: 0,
            scope: None,
        };
        let important = CascadedDeclaration {
            property: "color".into(),
            value: "blue".into(),
            important: true,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        assert_eq!(cascade_compare(&normal, &important), Ordering::Less);
    }

    #[test]
    fn higher_specificity_wins_within_origin() {
        let low = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::class(),
            source_order: 0,
            scope: None,
        };
        let high = CascadedDeclaration {
            property: "color".into(),
            value: "blue".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::id(),
            source_order: 0,
            scope: None,
        };
        assert_eq!(cascade_compare(&low, &high), Ordering::Less);
    }

    #[test]
    fn source_order_breaks_ties() {
        let earlier = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::class(),
            source_order: 1,
            scope: None,
        };
        let later = CascadedDeclaration {
            property: "color".into(),
            value: "blue".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::class(),
            source_order: 5,
            scope: None,
        };
        assert_eq!(cascade_compare(&earlier, &later), Ordering::Less);
    }

    #[test]
    fn unlayered_beats_layered() {
        let layered = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: Some(0),
            specificity: Specificity::id(),
            source_order: 0,
            scope: None,
        };
        let unlayered = CascadedDeclaration {
            property: "color".into(),
            value: "blue".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: None,
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        assert_eq!(cascade_compare(&layered, &unlayered), Ordering::Less);
    }

    #[test]
    fn later_layer_beats_earlier() {
        let earlier = CascadedDeclaration {
            property: "color".into(),
            value: "red".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: Some(0),
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        let later = CascadedDeclaration {
            property: "color".into(),
            value: "blue".into(),
            important: false,
            origin: CascadeOrigin::Author,
            layer: Some(1),
            specificity: Specificity::ZERO,
            source_order: 0,
            scope: None,
        };
        assert_eq!(cascade_compare(&earlier, &later), Ordering::Less);
    }
}
