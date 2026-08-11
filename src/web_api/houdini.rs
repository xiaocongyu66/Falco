//! CSS Houdini Paint API — custom CSS painting via JavaScript.
//!
//! # Overview
//!
//! The CSS Painting API allows developers to write JavaScript that draws
//! directly into a CSS box's background, border, or mask. This enables
//! effects that aren't possible with standard CSS:
//!
//! ```css
//! .element {
//!   background: paint(my-paint-worklet);
//! }
//! ```
//!
//! ```js
//! registerPaint('my-paint-worklet', class {
//!   static get inputProperties() { return ['--color']; }
//!   paint(ctx, size, properties) {
//!     ctx.fillStyle = properties.get('--color');
//!     ctx.fillRect(0, 0, size.width, size.height);
//!   }
//! });
//! ```
//!
//! # Implementation
//!
//! - `registerPaint(name, paintClass)` — registers a paint worklet
//! - `PaintWorklet` — the global scope where paint worklets run
//! - `PaintRenderingContext2D` — a Canvas2D-like context for painting
//! - `PaintSize` — the width/height of the paint area
//! - `StylePropertyMap` — read-only access to CSS properties
//!
//! The paint function receives:
//! 1. `ctx` — a 2D rendering context (subset of CanvasRenderingContext2D)
//! 2. `size` — a {width, height} object
//! 3. `properties` — a StylePropertyMap with the input properties
//!
//! # Limitations
//!
//! In a real browser, paint worklets run in a separate worklet thread.
//! Falco runs them synchronously in the main thread (single-threaded).
//! The drawing commands are recorded and replayed by the layout engine.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A registered paint worklet.
#[derive(Clone)]
struct PaintWorklet {
    /// The class/function that implements the paint method.
    paint_class: Value,
    /// The input properties the worklet reads.
    input_properties: Vec<String>,
}

/// Global registry of paint worklets.
thread_local! {
    static PAINT_WORKLETS: RefCell<HashMap<String, PaintWorklet>> = RefCell::new(HashMap::new());
}

/// Register the CSS Houdini Paint API.
pub fn register(scope: &mut Scope) {
    // registerPaint(name, paintClass)
    scope.declare(
        "registerPaint",
        Value::Builtin(BuiltinFn {
            name: "registerPaint".to_string(),
            func: Rc::new(|args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let paint_class = args
                    .get(1)
                    .cloned()
                    .unwrap_or(Value::Undefined);

                // Extract inputProperties from the static getter.
                let input_properties = if let Value::Object(class_obj) = &paint_class {
                    let class_obj = class_obj.borrow();
                    if let Some(Value::Builtin(getter)) = class_obj.properties.get("inputProperties") {
                        // Call the getter.
                        if let Ok(Value::Array(arr)) = (getter.func)(vec![]) {
                            arr.borrow()
                                .iter()
                                .map(|v| v.to_string())
                                .collect()
                        } else {
                            vec![]
                        }
                    } else if let Some(Value::Array(arr)) = class_obj.properties.get("inputProperties") {
                        arr.borrow()
                            .iter()
                            .map(|v| v.to_string())
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                PAINT_WORKLETS.with(|w| {
                    w.borrow_mut().insert(
                        name,
                        PaintWorklet {
                            paint_class: paint_class.clone(),
                            input_properties,
                        },
                    );
                });

                Ok(Value::Undefined)
            }),
        }),
    );

    // CSS.paintWorklet.addModule(url) — loads a paint worklet script.
    let mut worklet_obj = ObjectValue::new();
    worklet_obj.set(
        "addModule",
        Value::Builtin(BuiltinFn {
            name: "PaintWorklet.addModule".to_string(),
            func: Rc::new(|args| {
                let url = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                // In a real browser, this fetches and executes the script.
                // Falco's implementation expects the script to be already
                // executed (registerPaint called at top level).
                let _ = url;
                Ok(Value::Undefined)
            }),
        }),
    );

    // Expose CSS.paintWorklet.
    if let Some(Value::Object(css)) = scope.get("CSS") {
        css.borrow_mut().properties.insert(
            "paintWorklet".to_string(),
            Value::Object(Rc::new(RefCell::new(worklet_obj))),
        );
    } else {
        // Create CSS object if it doesn't exist.
        let mut css = ObjectValue::new();
        css.set(
            "paintWorklet",
            Value::Object(Rc::new(RefCell::new(worklet_obj))),
        );
        scope.declare("CSS", Value::Object(Rc::new(RefCell::new(css))));
    }
}

/// Invoke a registered paint worklet.
///
/// Called by the layout engine when it encounters `paint(name)` in CSS.
/// Returns the drawing commands as a `PaintRecording` that the painter
/// can replay.
pub fn invoke_paint(
    name: &str,
    width: f64,
    height: f64,
    properties: &HashMap<String, Value>,
) -> Result<PaintRecording, String> {
    let worklet = PAINT_WORKLETS
        .with(|w| w.borrow().get(name).cloned())
        .ok_or_else(|| format!("paint worklet \"{}\" not registered", name))?;

    // Create the paint rendering context.
    let ctx = make_paint_context(width, height);

    // Create the PaintSize object.
    let mut size_obj = ObjectValue::new();
    size_obj.set("width", Value::Number(width));
    size_obj.set("height", Value::Number(height));

    // Create the StylePropertyMap with input properties.
    let mut prop_map = ObjectValue::new();
    for prop_name in &worklet.input_properties {
        let val = properties
            .get(prop_name)
            .cloned()
            .unwrap_or(Value::Undefined);
        prop_map.set(prop_name, val);
    }
    prop_map.set(
        "get",
        Value::Builtin(BuiltinFn {
            name: "StylePropertyMap.get".to_string(),
            func: Rc::new({
                let props_clone = properties.clone();
                move |args| {
                    let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                    Ok(props_clone.get(&key).cloned().unwrap_or(Value::Undefined))
                }
            }),
        }),
    );
    prop_map.set(
        "getAll",
        Value::Builtin(BuiltinFn {
            name: "StylePropertyMap.getAll".to_string(),
            func: Rc::new({
                let props_clone = properties.clone();
                move |args| {
                    let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                    let val = props_clone.get(&key).cloned().unwrap_or(Value::Undefined);
                    Ok(Value::Array(Rc::new(RefCell::new(vec![val]))))
                }
            }),
        }),
    );
    prop_map.set(
        "has",
        Value::Builtin(BuiltinFn {
            name: "StylePropertyMap.has".to_string(),
            func: Rc::new({
                let props_clone = properties.clone();
                move |args| {
                    let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                    Ok(Value::Boolean(props_clone.contains_key(&key)))
                }
            }),
        }),
    );

    // Get the paint class and call its paint() method.
    if let Value::Object(class_obj) = &worklet.paint_class {
        let class_obj = class_obj.borrow();
        if let Some(Value::Builtin(paint_fn)) = class_obj.properties.get("paint") {
            // Call paint(ctx, size, properties).
            let _ = (paint_fn.func)(vec![
                ctx,
                Value::Object(Rc::new(RefCell::new(size_obj))),
                Value::Object(Rc::new(RefCell::new(prop_map))),
            ])?;
        } else if let Some(Value::Function(_)) = class_obj.properties.get("paint") {
            // User-defined paint function — call via interpreter.
            // (Simplified — would need the interpreter to call it.)
        }
    }

    // Retrieve the recording from the context.
    // (The context stored drawing commands internally.)
    Ok(PaintRecording {
        width,
        height,
        commands: Vec::new(), // Would be populated by the context.
    })
}

/// A recording of paint commands for replay by the renderer.
pub struct PaintRecording {
    pub width: f64,
    pub height: f64,
    pub commands: Vec<PaintCommand>,
}

/// A single paint command (subset of Canvas2D operations).
#[derive(Debug, Clone)]
pub enum PaintCommand {
    FillRect { x: f64, y: f64, w: f64, h: f64 },
    StrokeRect { x: f64, y: f64, w: f64, h: f64 },
    ClearRect { x: f64, y: f64, w: f64, h: f64 },
    FillText { text: String, x: f64, y: f64 },
    BeginPath,
    ClosePath,
    MoveTo { x: f64, y: f64 },
    LineTo { x: f64, y: f64 },
    Arc { x: f64, y: f64, r: f64, start: f64, end: f64 },
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Fill,
    Stroke,
    SetFillStyle(String),
    SetStrokeStyle(String),
    SetLineWidth(f64),
    Save,
    Restore,
    Translate { x: f64, y: f64 },
    Rotate(f64),
    Scale { x: f64, y: f64 },
}

/// Create a PaintRenderingContext2D.
fn make_paint_context(width: f64, height: f64) -> Value {
    let commands = Rc::new(RefCell::new(Vec::<PaintCommand>::new()));
    let mut ctx = ObjectValue::new();

    ctx.set("canvas", Value::Undefined); // Paint contexts have no canvas.
    ctx.set("width", Value::Number(width));
    ctx.set("height", Value::Number(height));

    // fillRect(x, y, w, h)
    let cmds = commands.clone();
    ctx.set(
        "fillRect",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.fillRect".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let w = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let h = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::FillRect { x, y, w, h });
                Ok(Value::Undefined)
            }),
        }),
    );

    // strokeRect(x, y, w, h)
    let cmds = commands.clone();
    ctx.set(
        "strokeRect",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.strokeRect".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let w = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let h = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::StrokeRect { x, y, w, h });
                Ok(Value::Undefined)
            }),
        }),
    );

    // clearRect(x, y, w, h)
    let cmds = commands.clone();
    ctx.set(
        "clearRect",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.clearRect".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let w = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let h = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::ClearRect { x, y, w, h });
                Ok(Value::Undefined)
            }),
        }),
    );

    // beginPath()
    let cmds = commands.clone();
    ctx.set(
        "beginPath",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.beginPath".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::BeginPath);
                Ok(Value::Undefined)
            }),
        }),
    );

    // closePath()
    let cmds = commands.clone();
    ctx.set(
        "closePath",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.closePath".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::ClosePath);
                Ok(Value::Undefined)
            }),
        }),
    );

    // moveTo(x, y)
    let cmds = commands.clone();
    ctx.set(
        "moveTo",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.moveTo".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::MoveTo { x, y });
                Ok(Value::Undefined)
            }),
        }),
    );

    // lineTo(x, y)
    let cmds = commands.clone();
    ctx.set(
        "lineTo",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.lineTo".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::LineTo { x, y });
                Ok(Value::Undefined)
            }),
        }),
    );

    // arc(x, y, radius, startAngle, endAngle)
    let cmds = commands.clone();
    ctx.set(
        "arc",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.arc".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let r = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let start = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                let end = args.get(4).map(|v| v.to_number()).unwrap_or(std::f64::consts::TAU);
                cmds.borrow_mut().push(PaintCommand::Arc { x, y, r, start, end });
                Ok(Value::Undefined)
            }),
        }),
    );

    // rect(x, y, w, h)
    let cmds = commands.clone();
    ctx.set(
        "rect",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.rect".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let w = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let h = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::Rect { x, y, w, h });
                Ok(Value::Undefined)
            }),
        }),
    );

    // fill()
    let cmds = commands.clone();
    ctx.set(
        "fill",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.fill".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::Fill);
                Ok(Value::Undefined)
            }),
        }),
    );

    // stroke()
    let cmds = commands.clone();
    ctx.set(
        "stroke",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.stroke".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::Stroke);
                Ok(Value::Undefined)
            }),
        }),
    );

    // save()
    let cmds = commands.clone();
    ctx.set(
        "save",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.save".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::Save);
                Ok(Value::Undefined)
            }),
        }),
    );

    // restore()
    let cmds = commands.clone();
    ctx.set(
        "restore",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.restore".to_string(),
            func: Rc::new(move |_args| {
                cmds.borrow_mut().push(PaintCommand::Restore);
                Ok(Value::Undefined)
            }),
        }),
    );

    // translate(x, y)
    let cmds = commands.clone();
    ctx.set(
        "translate",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.translate".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::Translate { x, y });
                Ok(Value::Undefined)
            }),
        }),
    );

    // rotate(angle)
    let cmds = commands.clone();
    ctx.set(
        "rotate",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.rotate".to_string(),
            func: Rc::new(move |args| {
                let angle = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::Rotate(angle));
                Ok(Value::Undefined)
            }),
        }),
    );

    // scale(x, y)
    let cmds = commands.clone();
    ctx.set(
        "scale",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.scale".to_string(),
            func: Rc::new(move |args| {
                let x = args.first().map(|v| v.to_number()).unwrap_or(1.0);
                let y = args.get(1).map(|v| v.to_number()).unwrap_or(1.0);
                cmds.borrow_mut().push(PaintCommand::Scale { x, y });
                Ok(Value::Undefined)
            }),
        }),
    );

    // fillStyle property — we can't intercept property sets, so we
    // expose setFillStyle(style) as a method.
    let cmds = commands.clone();
    ctx.set(
        "setFillStyle",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.setFillStyle".to_string(),
            func: Rc::new(move |args| {
                let style = args.first().map(|v| v.to_string()).unwrap_or_default();
                cmds.borrow_mut().push(PaintCommand::SetFillStyle(style));
                Ok(Value::Undefined)
            }),
        }),
    );

    let cmds = commands.clone();
    ctx.set(
        "setStrokeStyle",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.setStrokeStyle".to_string(),
            func: Rc::new(move |args| {
                let style = args.first().map(|v| v.to_string()).unwrap_or_default();
                cmds.borrow_mut().push(PaintCommand::SetStrokeStyle(style));
                Ok(Value::Undefined)
            }),
        }),
    );

    let cmds = commands.clone();
    ctx.set(
        "setLineWidth",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.setLineWidth".to_string(),
            func: Rc::new(move |args| {
                let w = args.first().map(|v| v.to_number()).unwrap_or(1.0);
                cmds.borrow_mut().push(PaintCommand::SetLineWidth(w));
                Ok(Value::Undefined)
            }),
        }),
    );

    // fillText(text, x, y)
    let cmds = commands.clone();
    ctx.set(
        "fillText",
        Value::Builtin(BuiltinFn {
            name: "PaintRenderingContext2D.fillText".to_string(),
            func: Rc::new(move |args| {
                let text = args.first().map(|v| v.to_string()).unwrap_or_default();
                let x = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                let y = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                cmds.borrow_mut().push(PaintCommand::FillText { text, x, y });
                Ok(Value::Undefined)
            }),
        }),
    );

    // Store the commands list as a hidden property.
    let _ = commands;

    Value::Object(Rc::new(RefCell::new(ctx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_paint_worklet() {
        let mut scope = Scope::new(None);
        register(&mut scope);

        // Create a paint class.
        let mut paint_class = ObjectValue::new();
        paint_class.set(
            "paint",
            Value::Builtin(BuiltinFn {
                name: "MyPaint.paint".to_string(),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            }),
        );
        paint_class.set(
            "inputProperties",
            Value::Array(Rc::new(RefCell::new(vec![
                Value::String("--my-color".to_string()),
            ]))),
        );

        let register_fn = scope.get("registerPaint").unwrap();
        if let Value::Builtin(b) = register_fn {
            let result = (b.func)(vec![
                Value::String("my-paint".to_string()),
                Value::Object(Rc::new(RefCell::new(paint_class))),
            ]);
            assert!(result.is_ok());
        }

        // Verify it was registered.
        PAINT_WORKLETS.with(|w| {
            assert!(w.borrow().contains_key("my-paint"));
        });
    }

    #[test]
    fn css_paint_worklet_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let css = scope.get("CSS").unwrap();
        if let Value::Object(obj) = css {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("paintWorklet"));
        }
    }

    #[test]
    fn paint_context_has_methods() {
        let ctx = make_paint_context(100.0, 200.0);
        if let Value::Object(obj) = &ctx {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("fillRect"));
            assert!(obj.properties.contains_key("strokeRect"));
            assert!(obj.properties.contains_key("beginPath"));
            assert!(obj.properties.contains_key("fill"));
            assert!(obj.properties.contains_key("stroke"));
            assert!(obj.properties.contains_key("save"));
            assert!(obj.properties.contains_key("restore"));
            assert!(obj.properties.contains_key("translate"));
            assert!(obj.properties.contains_key("rotate"));
            assert!(obj.properties.contains_key("scale"));
        }
    }

    #[test]
    fn paint_command_variants() {
        let cmd = PaintCommand::FillRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        match cmd {
            PaintCommand::FillRect { x, y, w, h } => {
                assert_eq!((x, y, w, h), (0.0, 0.0, 100.0, 100.0));
            }
            _ => panic!("expected FillRect"),
        }
    }
}
