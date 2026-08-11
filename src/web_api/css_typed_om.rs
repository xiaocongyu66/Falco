//! CSS Typed OM — typed CSS values.
//!
//! # Overview
//!
//! The CSS Typed OM provides JavaScript objects that represent CSS values
//! in a type-safe way, replacing the string-based `element.style` API.
//!
//! ```js
//! // Old (CSSOM):
//! element.style.opacity = "0.5";
//!
//! // New (Typed OM):
//! element.attributeStyleMap.set("opacity", CSS.number(0.5));
//! let val = element.attributeStyleMap.get("opacity"); // CSSUnitValue
//! val.value;  // 0.5
//! val.unit;   // "number"
//! ```
//!
//! # Types
//!
//! - `CSSStyleValue` — base class for all typed CSS values
//! - `CSSUnitValue` — a number + unit (e.g., "10px", "50%")
//! - `CSSKeywordValue` — a keyword (e.g., "auto", "none")
//! - `CSSMathSum` — sum of values (e.g., "10px + 20px")
//! - `CSSMathProduct` — product of values
//! - `CSSMathNegate` — negation
//! - `CSSMathInvert` — reciprocal
//! - `CSSPositionValue` — a position (x, y)
//! - `CSSImageValue` — an image URL
//! - `CSSUnparsedValue` — a value that couldn't be parsed
//!
//! # Usage
//!
//! These objects are accessed via `element.attributeStyleMap` and
//! `element.computedStyleMap()`.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the CSS Typed OM API.
pub fn register(scope: &mut Scope) {
    // CSS namespace object.
    let mut css_obj = ObjectValue::new();

    // CSS.number(value) → CSSUnitValue { value, unit: "number" }
    css_obj.set(
        "number",
        Value::Builtin(BuiltinFn {
            name: "CSS.number".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                make_unit_value(value, "number")
            }),
        }),
    );

    // CSS.px(value) → CSSUnitValue { value, unit: "px" }
    css_obj.set(
        "px",
        Value::Builtin(BuiltinFn {
            name: "CSS.px".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                make_unit_value(value, "px")
            }),
        }),
    );

    // CSS.percent(value) → CSSUnitValue { value, unit: "percent" }
    css_obj.set(
        "percent",
        Value::Builtin(BuiltinFn {
            name: "CSS.percent".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                make_unit_value(value, "percent")
            }),
        }),
    );

    // CSS.em(value) → CSSUnitValue { value, unit: "em" }
    css_obj.set(
        "em",
        Value::Builtin(BuiltinFn {
            name: "CSS.em".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                make_unit_value(value, "em")
            }),
        }),
    );

    // CSS.rem(value)
    css_obj.set(
        "rem",
        Value::Builtin(BuiltinFn {
            name: "CSS.rem".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                make_unit_value(value, "rem")
            }),
        }),
    );

    // CSS.vw(value), CSS.vh(value), CSS.vmin(value), CSS.vmax(value)
    for unit in &["vw", "vh", "vmin", "vmax"] {
        let unit_str = unit.to_string();
        css_obj.set(
            unit,
            Value::Builtin(BuiltinFn {
                name: format!("CSS.{}", unit),
                func: Rc::new(move |args| {
                    let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                    make_unit_value(value, &unit_str)
                }),
            }),
        );
    }

    // CSS.deg(value), CSS.rad(value), CSS.turn(value)
    for unit in &["deg", "rad", "turn"] {
        let unit_str = unit.to_string();
        css_obj.set(
            unit,
            Value::Builtin(BuiltinFn {
                name: format!("CSS.{}", unit),
                func: Rc::new(move |args| {
                    let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                    make_unit_value(value, &unit_str)
                }),
            }),
        );
    }

    // CSS.s(value), CSS.ms(value)
    for unit in &["s", "ms"] {
        let unit_str = unit.to_string();
        css_obj.set(
            unit,
            Value::Builtin(BuiltinFn {
                name: format!("CSS.{}", unit),
                func: Rc::new(move |args| {
                    let value = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                    make_unit_value(value, &unit_str)
                }),
            }),
        );
    }

    // CSS.keyword(value) → CSSKeywordValue
    css_obj.set(
        "keyword",
        Value::Builtin(BuiltinFn {
            name: "CSS.keyword".to_string(),
            func: Rc::new(|args| {
                let value = args.first().map(|v| v.to_string()).unwrap_or_default();
                let mut obj = ObjectValue::new();
                obj.set("value", Value::String(value.clone()));
                obj.set(
                    "toString",
                    Value::Builtin(BuiltinFn {
                        name: "CSSKeywordValue.toString".to_string(),
                        func: Rc::new(move |_args| Ok(Value::String(value.clone()))),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSS.image(url) → CSSImageValue
    css_obj.set(
        "image",
        Value::Builtin(BuiltinFn {
            name: "CSS.image".to_string(),
            func: Rc::new(|args| {
                let url = args.first().map(|v| v.to_string()).unwrap_or_default();
                let mut obj = ObjectValue::new();
                obj.set("url", Value::String(url));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSSPositionValue
    css_obj.set(
        "PositionValue",
        Value::Builtin(BuiltinFn {
            name: "CSSPositionValue".to_string(),
            func: Rc::new(|args| {
                let x = args.first().cloned().unwrap_or(Value::Undefined);
                let y = args.get(1).cloned().unwrap_or(Value::Undefined);
                let mut obj = ObjectValue::new();
                obj.set("x", x);
                obj.set("y", y);
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSSMathSum
    css_obj.set(
        "MathSum",
        Value::Builtin(BuiltinFn {
            name: "CSSMathSum".to_string(),
            func: Rc::new(|args| {
                let values: Vec<Value> = args.into_iter().collect();
                let mut obj = ObjectValue::new();
                obj.set("operator", Value::String("sum".to_string()));
                obj.set("values", Value::Array(Rc::new(RefCell::new(values))));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSSMathProduct
    css_obj.set(
        "MathProduct",
        Value::Builtin(BuiltinFn {
            name: "CSSMathProduct".to_string(),
            func: Rc::new(|args| {
                let values: Vec<Value> = args.into_iter().collect();
                let mut obj = ObjectValue::new();
                obj.set("operator", Value::String("product".to_string()));
                obj.set("values", Value::Array(Rc::new(RefCell::new(values))));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSSMathNegate
    css_obj.set(
        "MathNegate",
        Value::Builtin(BuiltinFn {
            name: "CSSMathNegate".to_string(),
            func: Rc::new(|args| {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let mut obj = ObjectValue::new();
                obj.set("operator", Value::String("negate".to_string()));
                obj.set("value", value);
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSSMathInvert
    css_obj.set(
        "MathInvert",
        Value::Builtin(BuiltinFn {
            name: "CSSMathInvert".to_string(),
            func: Rc::new(|args| {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let mut obj = ObjectValue::new();
                obj.set("operator", Value::String("invert".to_string()));
                obj.set("value", value);
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // CSS.supports(property, value) — check if a CSS declaration is supported.
    css_obj.set(
        "supports",
        Value::Builtin(BuiltinFn {
            name: "CSS.supports".to_string(),
            func: Rc::new(|_args| {
                // Simplified — always return true.
                Ok(Value::Boolean(true))
            }),
        }),
    );

    // CSS.escape(string) — escape a string for use as a CSS identifier.
    css_obj.set(
        "escape",
        Value::Builtin(BuiltinFn {
            name: "CSS.escape".to_string(),
            func: Rc::new(|args| {
                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                let mut result = String::new();
                for c in s.chars() {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        result.push(c);
                    } else {
                        result.push_str(&format!("\\{:x}", c as u32));
                    }
                }
                Ok(Value::String(result))
            }),
        }),
    );

    scope.declare("CSS", Value::Object(Rc::new(RefCell::new(css_obj))));
}

/// Create a CSSUnitValue.
fn make_unit_value(value: f64, unit: &str) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("value", Value::Number(value));
    obj.set("unit", Value::String(unit.to_string()));

    let unit_clone = unit.to_string();
    let value_clone = value;
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "CSSUnitValue.toString".to_string(),
            func: Rc::new(move |_args| {
                let suffix = match unit_clone.as_str() {
                    "number" => "",
                    "percent" => "%",
                    other => other,
                };
                Ok(Value::String(format!("{}{}", value_clone, suffix)))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_number() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(num_fn)) = obj.properties.get("number") {
                let result = (num_fn.func)(vec![Value::Number(0.5)]).unwrap();
                if let Value::Object(unit_val) = result {
                    let uv = unit_val.borrow();
                    assert_eq!(uv.properties.get("value"), Some(&Value::Number(0.5)));
                    assert_eq!(uv.properties.get("unit"), Some(&Value::String("number".to_string())));
                }
            }
        }
    }

    #[test]
    fn css_px() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(px_fn)) = obj.properties.get("px") {
                let result = (px_fn.func)(vec![Value::Number(100.0)]).unwrap();
                if let Value::Object(unit_val) = result {
                    let uv = unit_val.borrow();
                    assert_eq!(uv.properties.get("value"), Some(&Value::Number(100.0)));
                    assert_eq!(uv.properties.get("unit"), Some(&Value::String("px".to_string())));
                }
            }
        }
    }

    #[test]
    fn css_percent() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(pct_fn)) = obj.properties.get("percent") {
                let result = (pct_fn.func)(vec![Value::Number(50.0)]).unwrap();
                if let Value::Object(unit_val) = result {
                    let uv = unit_val.borrow();
                    assert_eq!(uv.properties.get("unit"), Some(&Value::String("percent".to_string())));
                }
            }
        }
    }

    #[test]
    fn css_keyword() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(kw_fn)) = obj.properties.get("keyword") {
                let result = (kw_fn.func)(vec![Value::String("auto".to_string())]).unwrap();
                if let Value::Object(kw) = result {
                    let kw = kw.borrow();
                    assert_eq!(kw.properties.get("value"), Some(&Value::String("auto".to_string())));
                }
            }
        }
    }

    #[test]
    fn css_escape() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(esc_fn)) = obj.properties.get("escape") {
                let result = (esc_fn.func)(vec![Value::String("hello world".to_string())]).unwrap();
                if let Value::String(s) = result {
                    // Space should be escaped.
                    assert!(s.contains("\\20"));
                }
            }
        }
    }

    #[test]
    fn css_unit_value_to_string() {
        let v = make_unit_value(42.0, "px").unwrap();
        if let Value::Object(obj) = &v {
            let obj = obj.borrow();
            if let Some(Value::Builtin(ts_fn)) = obj.properties.get("toString") {
                let result = (ts_fn.func)(vec![]).unwrap();
                assert_eq!(result, Value::String("42px".to_string()));
            }
        }
    }

    #[test]
    fn css_math_sum() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            if let Some(Value::Builtin(sum_fn)) = obj.properties.get("MathSum") {
                let result = (sum_fn.func)(vec![
                    make_unit_value(10.0, "px").unwrap(),
                    make_unit_value(20.0, "px").unwrap(),
                ]).unwrap();
                if let Value::Object(sum) = result {
                    let sum = sum.borrow();
                    assert_eq!(sum.properties.get("operator"), Some(&Value::String("sum".to_string())));
                }
            }
        }
    }
}
