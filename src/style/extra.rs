//! New CSS properties added in the major update:
//! - text-decoration (underline, line-through, none)
//! - text-transform (uppercase, lowercase, capitalize)
//! - letter-spacing, word-spacing
//! - list-style (disc, circle, square, none)
//! - vertical-align (top, middle, bottom, baseline)
//! - visibility (visible, hidden)

use crate::css::Color;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    None,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

/// Additional computed style properties (the "v2" additions).
/// These are stored separately to avoid bloating ComputedStyle further.
#[derive(Debug, Clone, Default)]
pub struct ExtraStyle {
    pub text_decoration: TextDecoration,
    pub text_decoration_color: Option<Color>,
    pub text_transform: TextTransform,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub vertical_align: VerticalAlign,
    pub visibility: Visibility,
    pub cursor: CursorType,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
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
}

impl ExtraStyle {
    /// Parse a CSS property name + value into an ExtraStyle update.
    /// Returns true if the property was recognized.
    pub fn apply(&mut self, prop: &str, val: &crate::css::Value) -> bool {
        match prop {
            "text-decoration" => match val {
                crate::css::Value::Keyword(k) => {
                    self.text_decoration = match k.as_str() {
                        "underline" => TextDecoration::Underline,
                        "line-through" => TextDecoration::LineThrough,
                        "overline" => TextDecoration::Overline,
                        _ => TextDecoration::None,
                    };
                    true
                }
                _ => false,
            },
            "text-decoration-color" => {
                if let crate::css::Value::Color(c) = val {
                    self.text_decoration_color = Some(*c);
                    true
                } else {
                    false
                }
            }
            "text-transform" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.text_transform = match k.as_str() {
                        "uppercase" => TextTransform::Uppercase,
                        "lowercase" => TextTransform::Lowercase,
                        "capitalize" => TextTransform::Capitalize,
                        _ => TextTransform::None,
                    };
                    true
                } else {
                    false
                }
            }
            "letter-spacing" => {
                if let crate::css::Value::Length(n, _) = val {
                    self.letter_spacing = *n;
                    true
                } else {
                    false
                }
            }
            "word-spacing" => {
                if let crate::css::Value::Length(n, _) = val {
                    self.word_spacing = *n;
                    true
                } else {
                    false
                }
            }
            "list-style" | "list-style-type" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.list_style_type = match k.as_str() {
                        "disc" => ListStyleType::Disc,
                        "circle" => ListStyleType::Circle,
                        "square" => ListStyleType::Square,
                        "decimal" => ListStyleType::Decimal,
                        "none" => ListStyleType::None,
                        _ => ListStyleType::Disc,
                    };
                    true
                } else {
                    false
                }
            }
            "list-style-position" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.list_style_position = match k.as_str() {
                        "inside" => ListStylePosition::Inside,
                        _ => ListStylePosition::Outside,
                    };
                    true
                } else {
                    false
                }
            }
            "vertical-align" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.vertical_align = match k.as_str() {
                        "top" => VerticalAlign::Top,
                        "middle" => VerticalAlign::Middle,
                        "bottom" => VerticalAlign::Bottom,
                        _ => VerticalAlign::Baseline,
                    };
                    true
                } else {
                    false
                }
            }
            "visibility" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.visibility = match k.as_str() {
                        "hidden" => Visibility::Hidden,
                        _ => Visibility::Visible,
                    };
                    true
                } else {
                    false
                }
            }
            "cursor" => {
                if let crate::css::Value::Keyword(k) = val {
                    self.cursor = match k.as_str() {
                        "pointer" => CursorType::Pointer,
                        "text" => CursorType::Text,
                        "wait" => CursorType::Wait,
                        "crosshair" => CursorType::Crosshair,
                        "not-allowed" => CursorType::NotAllowed,
                        _ => CursorType::Default,
                    };
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

/// Apply text-transform to a string.
pub fn apply_text_transform(text: &str, transform: TextTransform) -> String {
    match transform {
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut capitalize_next = true;
            for c in text.chars() {
                if c.is_whitespace() {
                    capitalize_next = true;
                    result.push(c);
                } else if capitalize_next {
                    for u in c.to_uppercase() {
                        result.push(u);
                    }
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
            result
        }
        TextTransform::None => text.to_string(),
    }
}

/// Get the bullet character for a list-style-type.
pub fn list_bullet(style: ListStyleType, index: usize) -> String {
    match style {
        ListStyleType::Disc => "•".to_string(),
        ListStyleType::Circle => "○".to_string(),
        ListStyleType::Square => "■".to_string(),
        ListStyleType::Decimal => format!("{}. ", index + 1),
        ListStyleType::None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_transform_uppercase() {
        assert_eq!(
            apply_text_transform("hello", TextTransform::Uppercase),
            "HELLO"
        );
    }

    #[test]
    fn test_text_transform_capitalize() {
        assert_eq!(
            apply_text_transform("hello world", TextTransform::Capitalize),
            "Hello World"
        );
    }

    #[test]
    fn test_list_bullet() {
        assert_eq!(list_bullet(ListStyleType::Disc, 0), "•");
        assert_eq!(list_bullet(ListStyleType::Decimal, 0), "1. ");
        assert_eq!(list_bullet(ListStyleType::Decimal, 2), "3. ");
        assert_eq!(list_bullet(ListStyleType::None, 0), "");
    }
}
