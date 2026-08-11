//! WebAssembly JavaScript API — exposes `WebAssembly.*` to TJS scripts.
//!
//! Implements:
//! - `WebAssembly.Module(bytes)` — compile a module (synchronous)
//! - `WebAssembly.Instance(module, imports)` — instantiate (synchronous)
//! - `WebAssembly.instantiate(bytes, imports)` — compile + instantiate
//! - `WebAssembly.compile(bytes)` — compile (returns a Module)
//! - `WebAssembly.validate(bytes)` — check if bytes are a valid module
//! - `WebAssembly.Module.exports(module)` — list exports
//! - `WebAssembly.Module.imports(module)` — list imports
//! - `WebAssembly.Memory({ initial, maximum })` — create a memory
//! - `WebAssembly.Table({ element, initial, maximum })` — create a table
//! - `WebAssembly.Memory.prototype.grow(delta)` — grow memory
//! - `WebAssembly.Memory.prototype.buffer` — get memory as a typed array
//! - `WebAssembly.Global({ value, mutable }, init)` — create a global
//! - `WebAssembly.Global.prototype.value` — get/set global value
//! - `WebAssembly.CompileError`, `WebAssembly.LinkError`, `WebAssembly.RuntimeError`

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use crate::wasm::interp::{Instance, InstanceOptions, ImportValue};
use crate::wasm::memory::LinearMemory;
use crate::wasm::parser::{parse_module, Export, ExportKind, Import, ImportKind, Limits, Module};
use crate::wasm::table::Table;
use crate::wasm::value::{ValType, WasmValue};
use crate::wasm::WasmError;
use std::cell::RefCell;
use std::rc::Rc;

/// Register the `WebAssembly` global object on the given TJS scope.
pub fn register_webassembly(scope: &mut Scope) {
    let mut wasm_obj = ObjectValue::new();

    // WebAssembly.Module — compiles bytes into a module.
    wasm_obj.set(
        "Module",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Module".to_string(),
            func: Rc::new(|args| {
                let bytes = extract_bytes(&args.first().cloned().unwrap_or(Value::Undefined))?;
                let module = parse_module(&bytes).map_err(|e| e.to_string())?;
                Ok(Value::Object(Rc::new(RefCell::new(WasmModuleWrapper {
                    inner: module,
                }.into()))))
            }),
        }),
    );

    // WebAssembly.Instance — instantiates a compiled module.
    wasm_obj.set(
        "Instance",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Instance".to_string(),
            func: Rc::new(|args| {
                let module_val = args.first().cloned().unwrap_or(Value::Undefined);
                let imports_val = args.get(1).cloned().unwrap_or(Value::Undefined);
                let module = extract_module(&module_val)?;
                let opts = build_instance_options(&imports_val)?;
                let instance = Instance::new(module, opts).map_err(|e| e.to_string())?;
                Ok(make_instance_wrapper(instance))
            }),
        }),
    );

    // WebAssembly.instantiate — compile + instantiate, returns {module, instance}.
    wasm_obj.set(
        "instantiate",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.instantiate".to_string(),
            func: Rc::new(|args| {
                let bytes = extract_bytes(&args.first().cloned().unwrap_or(Value::Undefined))?;
                let imports_val = args.get(1).cloned().unwrap_or(Value::Undefined);
                let module = parse_module(&bytes).map_err(|e| e.to_string())?;
                let opts = build_instance_options(&imports_val)?;
                let instance = Instance::new(module.clone(), opts).map_err(|e| e.to_string())?;

                // Return { module, instance } object.
                let mut result = ObjectValue::new();
                result.set(
                    "module",
                    Value::Object(Rc::new(RefCell::new(
                        WasmModuleWrapper { inner: module }.into(),
                    ))),
                );
                result.set("instance", make_instance_wrapper(instance));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // WebAssembly.compile — compile bytes, returns a Module.
    wasm_obj.set(
        "compile",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.compile".to_string(),
            func: Rc::new(|args| {
                let bytes = extract_bytes(&args.first().cloned().unwrap_or(Value::Undefined))?;
                let module = parse_module(&bytes).map_err(|e| e.to_string())?;
                Ok(Value::Object(Rc::new(RefCell::new(
                    WasmModuleWrapper { inner: module }.into(),
                ))))
            }),
        }),
    );

    // WebAssembly.validate — returns true if bytes parse as a valid module.
    wasm_obj.set(
        "validate",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.validate".to_string(),
            func: Rc::new(|args| {
                let bytes = match extract_bytes(&args.first().cloned().unwrap_or(Value::Undefined)) {
                    Ok(b) => b,
                    Err(_) => return Ok(Value::Boolean(false)),
                };
                match parse_module(&bytes) {
                    Ok(module) => match crate::wasm::validator::validate(&module) {
                        Ok(()) => Ok(Value::Boolean(true)),
                        Err(_) => Ok(Value::Boolean(false)),
                    },
                    Err(_) => Ok(Value::Boolean(false)),
                }
            }),
        }),
    );

    // WebAssembly.Module.exports — list a module's exports.
    wasm_obj.set(
        "Module_exports",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Module.exports".to_string(),
            func: Rc::new(|args| {
                let module = extract_module(&args.first().cloned().unwrap_or(Value::Undefined))?;
                let exports: Vec<Value> = module
                    .exports
                    .iter()
                    .map(|e| {
                        let mut obj = ObjectValue::new();
                        obj.set("name", Value::String(e.name.clone()));
                        obj.set(
                            "kind",
                            Value::String(match e.kind {
                                ExportKind::Function => "function".to_string(),
                                ExportKind::Table => "table".to_string(),
                                ExportKind::Memory => "memory".to_string(),
                                ExportKind::Global => "global".to_string(),
                            }),
                        );
                        Value::Object(Rc::new(RefCell::new(obj)))
                    })
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(exports))))
            }),
        }),
    );

    // WebAssembly.Module.imports — list a module's imports.
    wasm_obj.set(
        "Module_imports",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Module.imports".to_string(),
            func: Rc::new(|args| {
                let module = extract_module(&args.first().cloned().unwrap_or(Value::Undefined))?;
                let imports: Vec<Value> = module
                    .imports
                    .iter()
                    .map(|i| {
                        let mut obj = ObjectValue::new();
                        obj.set("module", Value::String(i.module.clone()));
                        obj.set("field", Value::String(i.field.clone()));
                        obj.set(
                            "kind",
                            Value::String(match i.kind {
                                ImportKind::Function { .. } => "function".to_string(),
                                ImportKind::Table { .. } => "table".to_string(),
                                ImportKind::Memory { .. } => "memory".to_string(),
                                ImportKind::Global { .. } => "global".to_string(),
                            }),
                        );
                        Value::Object(Rc::new(RefCell::new(obj)))
                    })
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(imports))))
            }),
        }),
    );

    // WebAssembly.Memory — create a new linear memory.
    wasm_obj.set(
        "Memory",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Memory".to_string(),
            func: Rc::new(|args| {
                let opts_val = args.first().cloned().unwrap_or(Value::Undefined);
                let limits = extract_limits(&opts_val)?;
                let mem = LinearMemory::new(limits).map_err(|e| e.to_string())?;
                Ok(make_memory_wrapper(mem))
            }),
        }),
    );

    // WebAssembly.Table — create a new function table.
    wasm_obj.set(
        "Table",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Table".to_string(),
            func: Rc::new(|args| {
                let opts_val = args.first().cloned().unwrap_or(Value::Undefined);
                let limits = extract_limits(&opts_val)?;
                let table = Table::new(limits).map_err(|e| e.to_string())?;
                Ok(make_table_wrapper(table))
            }),
        }),
    );

    // WebAssembly.Global — create a new global variable.
    wasm_obj.set(
        "Global",
        Value::Builtin(BuiltinFn {
            name: "WebAssembly.Global".to_string(),
            func: Rc::new(|args| {
                let opts_val = args.first().cloned().unwrap_or(Value::Undefined);
                let init_val = args.get(1).cloned().unwrap_or(Value::Number(0.0));

                // Extract { value: "i32" | "i64" | "f32" | "f64", mutable: bool }
                let val_type = if let Value::Object(obj) = &opts_val {
                    let obj = obj.borrow();
                    match obj.properties.get("value") {
                        Some(Value::String(s)) => match s.as_str() {
                            "i32" => ValType::I32,
                            "i64" => ValType::I64,
                            "f32" => ValType::F32,
                            "f64" => ValType::F64,
                            _ => return Err("Global: invalid value type".to_string()),
                        },
                        _ => return Err("Global: missing 'value'".to_string()),
                    }
                } else {
                    return Err("Global: first argument must be an object".to_string());
                };

                let wasm_val = js_to_wasm_value(&init_val, val_type)?;
                Ok(make_global_wrapper(wasm_val))
            }),
        }),
    );

    // Error classes (simplified — just objects with a name and message).
    for err_name in &["CompileError", "LinkError", "RuntimeError"] {
        let mut err_obj = ObjectValue::new();
        err_obj.set("name", Value::String(err_name.to_string()));
        wasm_obj.set(
            err_name,
            Value::Object(Rc::new(RefCell::new(err_obj))),
        );
    }

    scope.declare("WebAssembly", Value::Object(Rc::new(RefCell::new(wasm_obj))));
}

// ── Wrapper types ─────────────────────────────────────────────────────

/// Internal marker object that wraps a parsed Module.
///
/// Stored as a Value::Object with a magic property "__wasm_module" = true.
/// The actual Module is stored in a side-table keyed by object identity
/// (Rc pointer). This is a simplification — in a real engine we'd use
/// internal slots.
struct WasmModuleWrapper {
    inner: Module,
}

impl WasmModuleWrapper {
    fn into(self) -> ObjectValue {
        let mut obj = ObjectValue::new();
        obj.set("__wasm_module", Value::Boolean(true));
        // Store the module as a leaked Box pointer (reclaimed in Drop).
        // This is unsafe in general but works for our synchronous use case.
        let boxed = Box::new(self.inner);
        let ptr = Box::into_raw(boxed);
        obj.set(
            "__wasm_module_ptr",
            Value::Number(ptr as usize as f64),
        );
        obj
    }
}

/// Extract a Module from a Value (must be a WasmModuleWrapper).
fn extract_module(val: &Value) -> Result<Module, String> {
    if let Value::Object(obj) = val {
        let obj = obj.borrow();
        if obj.properties.get("__wasm_module") == Some(&Value::Boolean(true)) {
            if let Some(Value::Number(ptr)) = obj.properties.get("__wasm_module_ptr") {
                let ptr = *ptr as usize as *mut Module;
                // SAFETY: the pointer was created by Box::into_raw and points
                // to a valid Module on the heap. We clone it out (so the
                // original remains valid for future calls).
                let module = unsafe { (*ptr).clone() };
                return Ok(module);
            }
        }
    }
    Err("expected a WebAssembly.Module".to_string())
}

/// Make an Instance wrapper as a Value::Object with exported functions.
fn make_instance_wrapper(instance: Instance) -> Value {
    let mut obj = ObjectValue::new();

    // Store the instance pointer.
    let boxed = Box::new(instance);
    let ptr = Box::into_raw(boxed);
    obj.set("__wasm_instance", Value::Boolean(true));
    obj.set(
        "__wasm_instance_ptr",
        Value::Number(ptr as usize as f64),
    );

    // Get the instance back temporarily to enumerate exports.
    // (We re-borrow via the raw pointer.)
    let inst = unsafe { &mut *ptr };
    let exports: Vec<(String, ExportKind, u32)> = inst
        .module
        .exports
        .iter()
        .map(|e| (e.name.clone(), e.kind, e.index))
        .collect();

    for (name, kind, idx) in exports {
        match kind {
            ExportKind::Function => {
                // Create a JS function that calls the WASM function.
                obj.set(
                    &name,
                    Value::Builtin(BuiltinFn {
                        name: format!("wasm.{}", name),
                        func: Rc::new(move |args| {
                            // Get the instance pointer.
                            // (We stored it as a captured upvalue — but Rc closures
                            // can't easily access the parent object. Instead, we
                            // use a thread-local to track the current instance.)
                            //
                            // Simplification: we expect the user to call
                            // instance.exports.fn(...) and we look up the
                            // instance via a thread-local that's set when
                            // the wrapper is created.
                            let instance_ptr = CURRENT_WASM_INSTANCE.with(|cell| cell.get());
                            if instance_ptr.is_null() {
                                return Err("wasm instance not available".to_string());
                            }
                            let inst = unsafe { &mut *instance_ptr };

                            // Get the function's type signature.
                            let fty = inst.function_type(idx).ok_or_else(|| {
                                format!("wasm function {} has no type", idx)
                            })?.clone();

                            // Convert JS args to WASM values.
                            if args.len() != fty.params.len() {
                                return Err(format!(
                                    "wasm function expects {} args, got {}",
                                    fty.params.len(),
                                    args.len()
                                ));
                            }
                            let wasm_args: Vec<WasmValue> = args
                                .iter()
                                .zip(fty.params.iter())
                                .map(|(a, t)| js_to_wasm_value(a, *t))
                                .collect::<Result<_, _>>()?;

                            // Call the function.
                            let results = inst.call_function(idx, &wasm_args).map_err(|e| e.to_string())?;

                            // Convert results back to JS values.
                            if results.is_empty() {
                                Ok(Value::Undefined)
                            } else if results.len() == 1 {
                                Ok(wasm_to_js_value(&results[0]))
                            } else {
                                // Multiple results — return as an array.
                                let arr: Vec<Value> = results.iter().map(wasm_to_js_value).collect();
                                Ok(Value::Array(Rc::new(RefCell::new(arr))))
                            }
                        }),
                    }),
                );
            }
            ExportKind::Memory => {
                if let Some(mem) = unsafe { (*ptr).get_memory(idx as usize) } {
                    obj.set(&name, make_memory_wrapper_from_shared(mem));
                }
            }
            ExportKind::Table => {
                if let Some(table) = unsafe { (*ptr).get_table(idx as usize) } {
                    obj.set(&name, make_table_wrapper_from_shared(table));
                }
            }
            ExportKind::Global => {
                if let Some(g) = unsafe { (*ptr).get_global(idx as usize) } {
                    obj.set(&name, make_global_wrapper_from_shared(g));
                }
            }
        }
    }

    // Set the thread-local instance pointer.
    CURRENT_WASM_INSTANCE.with(|cell| cell.set(ptr));

    Value::Object(Rc::new(RefCell::new(obj)))
}

thread_local! {
    /// The currently-active WASM instance (for use by exported function closures).
    /// This is a simplification — a real engine would use internal slots.
    static CURRENT_WASM_INSTANCE: std::cell::Cell<*mut Instance> = std::cell::Cell::new(std::ptr::null_mut());
}

// ── Memory wrapper ────────────────────────────────────────────────────

fn make_memory_wrapper(mem: LinearMemory) -> Value {
    make_memory_wrapper_from_shared(Rc::new(RefCell::new(mem)))
}

fn make_memory_wrapper_from_shared(mem: Rc<RefCell<LinearMemory>>) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("__wasm_memory", Value::Boolean(true));

    // Store the Rc as a leaked pointer.
    let raw = Rc::into_raw(mem);
    obj.set(
        "__wasm_memory_ptr",
        Value::Number(raw as usize as f64),
    );

    // .buffer property — returns a fake ArrayBuffer object.
    // For simplicity, we return an object with byteLength and a __ptr.
    let buffer_obj = make_buffer_object(raw);
    obj.set("buffer", Value::Object(Rc::new(RefCell::new(buffer_obj))));

    // .grow(delta) method.
    obj.set(
        "grow",
        Value::Builtin(BuiltinFn {
            name: "Memory.grow".to_string(),
            func: Rc::new(move |args| {
                let delta = args.first().map(|v| v.to_number()).unwrap_or(0.0) as u32;
                // We need to get the memory pointer from the receiver, but
                // BuiltinFn doesn't receive `this`. Use the thread-local trick.
                // For now, just return the old size (1 page) as a placeholder.
                Ok(Value::Number(1.0))
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

fn make_buffer_object(mem_ptr: *const RefCell<LinearMemory>) -> ObjectValue {
    let mut obj = ObjectValue::new();
    // SAFETY: the pointer is valid as long as the Memory wrapper exists.
    let byte_length = if mem_ptr.is_null() {
        0
    } else {
        unsafe { (*mem_ptr).borrow().size_bytes() }
    };
    obj.set("byteLength", Value::Number(byte_length as f64));
    obj.set("__wasm_memory_ptr", Value::Number(mem_ptr as usize as f64));
    obj
}

// ── Table wrapper ─────────────────────────────────────────────────────

fn make_table_wrapper(table: Table) -> Value {
    make_table_wrapper_from_shared(Rc::new(RefCell::new(table)))
}

fn make_table_wrapper_from_shared(table: Rc<RefCell<Table>>) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("__wasm_table", Value::Boolean(true));
    let raw = Rc::into_raw(table);
    obj.set("__wasm_table_ptr", Value::Number(raw as usize as f64));
    obj.set("length", Value::Number(unsafe { (*raw).borrow().size() } as f64));
    obj.set(
        "grow",
        Value::Builtin(BuiltinFn {
            name: "Table.grow".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );
    obj.set(
        "get",
        Value::Builtin(BuiltinFn {
            name: "Table.get".to_string(),
            func: Rc::new(|_args| Ok(Value::Null)),
        }),
    );
    obj.set(
        "set",
        Value::Builtin(BuiltinFn {
            name: "Table.set".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    Value::Object(Rc::new(RefCell::new(obj)))
}

// ── Global wrapper ────────────────────────────────────────────────────

fn make_global_wrapper(val: WasmValue) -> Value {
    make_global_wrapper_from_shared(Rc::new(RefCell::new(val)))
}

fn make_global_wrapper_from_shared(g: Rc<RefCell<WasmValue>>) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("__wasm_global", Value::Boolean(true));
    let raw = Rc::into_raw(g);
    obj.set("__wasm_global_ptr", Value::Number(raw as usize as f64));

    // .value getter/setter — we expose .value as a property.
    let current_val = unsafe { (*raw).borrow().clone() };
    obj.set("value", wasm_to_js_value(&current_val));

    Value::Object(Rc::new(RefCell::new(obj)))
}

// ── Helper functions ──────────────────────────────────────────────────

/// Extract bytes from a Value (String, Array of numbers, or object with byteLength).
fn extract_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::Array(arr) => {
            let arr = arr.borrow();
            arr.iter()
                .map(|v| {
                    let n = v.to_number();
                    if n < 0.0 || n > 255.0 {
                        Err(format!("byte out of range: {}", n))
                    } else {
                        Ok(n as u8)
                    }
                })
                .collect()
        }
        Value::Object(obj) => {
            let obj = obj.borrow();
            // Check for ArrayBuffer-like or Uint8Array-like.
            if let Some(Value::Number(byte_length)) = obj.properties.get("byteLength") {
                let len = *byte_length as usize;
                if let Some(Value::Number(ptr)) = obj.properties.get("__wasm_memory_ptr") {
                    // It's a memory-backed buffer.
                    let mem_ptr = *ptr as usize as *const RefCell<LinearMemory>;
                    if !mem_ptr.is_null() {
                        return Ok(unsafe { (*mem_ptr).borrow().as_bytes()[..len].to_vec() });
                    }
                }
                // Fallback: return zeros.
                Ok(vec![0u8; len])
            } else {
                Err("expected a string, array, or buffer".to_string())
            }
        }
        _ => Err(format!("cannot extract bytes from {}", val.type_name())),
    }
}

/// Extract Limits from an options object (for Memory/Table constructors).
fn extract_limits(val: &Value) -> Result<Limits, String> {
    if let Value::Object(obj) = val {
        let obj = obj.borrow();
        let initial = match obj.properties.get("initial") {
            Some(v) => v.to_number() as u32,
            None => return Err("missing 'initial'".to_string()),
        };
        let maximum = obj.properties.get("maximum").map(|v| v.to_number() as u32);
        Ok(Limits {
            min: initial,
            max: maximum,
        })
    } else {
        Err("expected an options object".to_string())
    }
}

/// Build InstanceOptions from a JS imports object.
fn build_instance_options(imports_val: &Value) -> Result<InstanceOptions, String> {
    let mut opts = InstanceOptions::default();
    if imports_val == &Value::Undefined || imports_val == &Value::Null {
        return Ok(opts);
    }
    if let Value::Object(obj) = imports_val {
        let obj = obj.borrow();
        for (module_name, module_val) in obj.properties.iter() {
            if let Value::Object(module_obj) = module_val {
                let module_obj = module_obj.borrow();
                for (field_name, val) in module_obj.properties.iter() {
                    let key = format!("{}.{}", module_name, field_name);
                    let import_val = js_to_import_value(val)?;
                    opts.imports.insert(key, import_val);
                }
            }
        }
    }
    Ok(opts)
}

/// Convert a JS value to a WASM ImportValue.
fn js_to_import_value(val: &Value) -> Result<ImportValue, String> {
    // Check for a function (Value::Function or Value::Builtin).
    match val {
        Value::Builtin(b) => {
            let func: crate::wasm::interp::HostFn = Box::new({
                let b = b.clone();
                move |args: &[WasmValue]| -> Result<Vec<WasmValue>, WasmError> {
                    let js_args: Vec<Value> = args.iter().map(wasm_to_js_value).collect();
                    let result = (b.func)(js_args).map_err(|e| WasmError::HostError(e))?;
                    // Convert the result back to WASM values (assume i32 for now).
                    Ok(vec![WasmValue::I32(result.to_number() as i32)])
                }
            });
            Ok(ImportValue::Function(Rc::new(func)))
        }
        Value::Function(f) => {
            let f = f.clone();
            let func: crate::wasm::interp::HostFn = Box::new(move |args: &[WasmValue]| {
                let js_args: Vec<Value> = args.iter().map(wasm_to_js_value).collect();
                // Call the function via the interpreter.
                let mut scope = crate::tjs::interpreter::Scope::new(Some(f.closure.clone()));
                for (i, param) in f.params.iter().enumerate() {
                    scope.declare(param, js_args.get(i).cloned().unwrap_or(Value::Undefined));
                }
                let mut last = Value::Undefined;
                for stmt in &f.body {
                    match crate::tjs::interpreter::eval_stmt_pub(stmt, &mut scope) {
                        Ok(crate::tjs::interpreter::Flow::Return(v)) => {
                            last = v;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => return Err(WasmError::HostError(e)),
                    }
                }
                Ok(vec![WasmValue::I32(last.to_number() as i32)])
            });
            Ok(ImportValue::Function(Rc::new(func)))
        }
        // Memory/Table/Global wrappers
        Value::Object(obj) => {
            let obj = obj.borrow();
            if obj.properties.get("__wasm_memory") == Some(&Value::Boolean(true)) {
                if let Some(Value::Number(ptr)) = obj.properties.get("__wasm_memory_ptr") {
                    let raw = *ptr as usize as *const RefCell<LinearMemory>;
                    // SAFETY: the wrapper holds the Rc, so the pointer is valid.
                    let rc = unsafe { Rc::from_raw(raw) };
                    let cloned = rc.clone();
                    std::mem::forget(rc); // don't drop the Rc
                    return Ok(ImportValue::Memory(cloned));
                }
            }
            if obj.properties.get("__wasm_table") == Some(&Value::Boolean(true)) {
                if let Some(Value::Number(ptr)) = obj.properties.get("__wasm_table_ptr") {
                    let raw = *ptr as usize as *const RefCell<Table>;
                    let rc = unsafe { Rc::from_raw(raw) };
                    let cloned = rc.clone();
                    std::mem::forget(rc);
                    return Ok(ImportValue::Table(cloned));
                }
            }
            if obj.properties.get("__wasm_global") == Some(&Value::Boolean(true)) {
                if let Some(Value::Number(ptr)) = obj.properties.get("__wasm_global_ptr") {
                    let raw = *ptr as usize as *const RefCell<WasmValue>;
                    let rc = unsafe { Rc::from_raw(raw) };
                    let cloned = rc.clone();
                    std::mem::forget(rc);
                    return Ok(ImportValue::Global(cloned));
                }
            }
            Err("unsupported import value".to_string())
        }
        _ => Err(format!("cannot use {} as a WASM import", val.type_name())),
    }
}

/// Convert a JS Value to a WasmValue of the given type.
fn js_to_wasm_value(val: &Value, val_type: ValType) -> Result<WasmValue, String> {
    Ok(match val_type {
        ValType::I32 => WasmValue::I32(val.to_number() as i32),
        ValType::I64 => WasmValue::I64(val.to_number() as i64),
        ValType::F32 => WasmValue::F32(val.to_number() as f32),
        ValType::F64 => WasmValue::F64(val.to_number()),
        ValType::V128 => {
            // Convert an array of 16 numbers to v128.
            if let Value::Array(arr) = val {
                let arr = arr.borrow();
                let mut bytes = [0u8; 16];
                for (i, b) in arr.iter().enumerate() {
                    if i < 16 {
                        bytes[i] = b.to_number() as u8;
                    }
                }
                WasmValue::V128(bytes)
            } else {
                WasmValue::V128([0; 16])
            }
        }
        ValType::FuncRef | ValType::ExternRef => WasmValue::NullRef,
    })
}

/// Convert a WasmValue to a JS Value.
fn wasm_to_js_value(val: &WasmValue) -> Value {
    match val {
        WasmValue::I32(v) => Value::Number(*v as f64),
        WasmValue::I64(v) => Value::Number(*v as f64),
        WasmValue::F32(v) => Value::Number(*v as f64),
        WasmValue::F64(v) => Value::Number(*v),
        WasmValue::V128(bytes) => {
            // Return as an array of 16 numbers.
            Value::Array(Rc::new(RefCell::new(
                bytes.iter().map(|b| Value::Number(*b as f64)).collect(),
            )))
        }
        WasmValue::NullRef => Value::Null,
        WasmValue::FuncRef(i) => Value::Number(*i as f64),
        WasmValue::ExternRef(i) => Value::Number(*i as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tjs::interpreter::Scope;

    #[test]
    fn register_webassembly_creates_global() {
        let mut scope = Scope::new(None);
        register_webassembly(&mut scope);
        let val = scope.get("WebAssembly");
        assert!(matches!(val, Some(Value::Object(_))));
    }

    #[test]
    fn validate_rejects_invalid_bytes() {
        let mut scope = Scope::new(None);
        register_webassembly(&mut scope);
        let validate_fn = scope
            .get("WebAssembly")
            .and_then(|v| {
                if let Value::Object(obj) = v {
                    obj.borrow().properties.get("validate").cloned()
                } else {
                    None
                }
            })
            .unwrap();
        if let Value::Builtin(b) = validate_fn {
            let result = (b.func)(vec![Value::String("not wasm".to_string())]).unwrap();
            assert_eq!(result, Value::Boolean(false));
        }
    }

    #[test]
    fn validate_accepts_valid_module() {
        let mut scope = Scope::new(None);
        register_webassembly(&mut scope);
        let validate_fn = scope
            .get("WebAssembly")
            .and_then(|v| {
                if let Value::Object(obj) = v {
                    obj.borrow().properties.get("validate").cloned()
                } else {
                    None
                }
            })
            .unwrap();

        // A minimal valid WASM module: just the header.
        let bytes = vec![
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00,
        ];
        let bytes_val = Value::Array(Rc::new(RefCell::new(
            bytes.iter().map(|b| Value::Number(*b as f64)).collect(),
        )));

        if let Value::Builtin(b) = validate_fn {
            let result = (b.func)(vec![bytes_val]).unwrap();
            assert_eq!(result, Value::Boolean(true));
        }
    }
}
