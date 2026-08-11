//! ES Modules (import/export) — basic implementation.
//!
//! # Supported Syntax
//!
//! ```js
//! // Named imports
//! import { foo, bar } from "./module.js";
//!
//! // Default import
//! import myDefault from "./module.js";
//!
//! // Namespace import
//! import * as mod from "./module.js";
//!
//! // Named exports
//! export function foo() {}
//! export const bar = 42;
//!
//! // Default export
//! export default class MyClass {}
//!
//! // Re-export
//! export { foo } from "./other.js";
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A loaded ES module.
struct LoadedModule {
    /// The module's namespace object (all exports).
    namespace: Value,
    /// Whether the module has been loaded.
    loaded: bool,
}

/// Global module registry (thread-local).
thread_local! {
    static MODULE_REGISTRY: RefCell<HashMap<String, LoadedModule>> =
        RefCell::new(HashMap::new());
}

/// Register a module in the global registry.
///
/// Call this before executing scripts that `import` from this module.
pub fn register_module(name: &str, exports: Value) {
    MODULE_REGISTRY.with(|reg| {
        reg.borrow_mut().insert(
            name.to_string(),
            LoadedModule {
                namespace: exports,
                loaded: true,
            },
        );
    });
}

/// Get a module's namespace object from the registry.
pub fn get_module(name: &str) -> Option<Value> {
    MODULE_REGISTRY.with(|reg| {
        reg.borrow()
            .get(name)
            .map(|m| m.namespace.clone())
    })
}

/// Pre-process a script that contains ES module syntax, converting
/// `import`/`export` statements into runtime calls.
///
/// This is a simplified transformer that handles the common cases:
///
/// ```js
/// // Before:
/// import { foo } from "./mod.js";
/// export const bar = 42;
///
/// // After:
/// var __mod_0 = __falco_import("./mod.js");
/// var foo = __mod_0.foo;
/// var __exports = {};
/// __exports.bar = 42;
/// __falco_export(__exports);
/// ```
pub fn transform_module_syntax(source: &str) -> String {
    let mut output = String::new();
    let mut import_count = 0;
    let mut has_exports = false;

    // Add the runtime helpers.
    output.push_str("var __falco_exports = {};\n");

    for line in source.lines() {
        let trimmed = line.trim();

        // Handle: import { foo, bar } from "./mod.js"
        if trimmed.starts_with("import {") && trimmed.contains(" from ") {
            let var_name = format!("__mod_{}", import_count);
            import_count += 1;

            // Extract the module path.
            let path = extract_module_path(trimmed);
            output.push_str(&format!(
                "var {} = __falco_require({});\n",
                var_name,
                quote_string(&path)
            ));

            // Extract the named imports.
            let names_str = trimmed
                .strip_prefix("import {")
                .and_then(|s| s.split("}").next())
                .unwrap_or("");
            for name in names_str.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    output.push_str(&format!("var {} = {}.{};\n", name, var_name, name));
                }
            }
            continue;
        }

        // Handle: import * as mod from "./mod.js" (must check before default import)
        if trimmed.starts_with("import * as ") && trimmed.contains(" from ") {
            let var_name = format!("__mod_{}", import_count);
            import_count += 1;

            let ns_name = trimmed
                .strip_prefix("import * as ")
                .and_then(|s| s.split(" from ").next())
                .unwrap_or("mod")
                .trim()
                .to_string();

            let path = extract_module_path(trimmed);
            output.push_str(&format!(
                "var {} = __falco_require({});\nvar {} = {};\n",
                var_name,
                quote_string(&path),
                ns_name,
                var_name
            ));
            continue;
        }

        // Handle: import defaultExport from "./mod.js"
        if trimmed.starts_with("import ") && trimmed.contains(" from ") && !trimmed.contains("{") && !trimmed.contains("* as") {
            let var_name = format!("__mod_{}", import_count);
            import_count += 1;

            let default_name = trimmed
                .strip_prefix("import ")
                .and_then(|s| s.split(" from ").next())
                .unwrap_or("default")
                .trim()
                .to_string();

            let path = extract_module_path(trimmed);
            output.push_str(&format!(
                "var {} = __falco_require({});\nvar {} = {}.default || {};\n",
                var_name,
                quote_string(&path),
                default_name,
                var_name,
                var_name
            ));
            continue;
        }

        // Handle: export const foo = ...
        if let Some(rest) = trimmed.strip_prefix("export const ") {
            has_exports = true;
            let rest = rest.trim();
            // Extract variable name.
            let var_name = rest.split('=').next().unwrap_or("").trim();
            output.push_str(&format!("var {} = {};\n__falco_exports.{} = {};\n",
                var_name, rest.trim_start_matches(var_name).trim_start_matches('=').trim(),
                var_name, var_name));
            continue;
        }

        // Handle: export let foo = ...
        if let Some(rest) = trimmed.strip_prefix("export let ") {
            has_exports = true;
            let rest = rest.trim();
            let var_name = rest.split('=').next().unwrap_or("").trim();
            output.push_str(&format!("var {} = {};\n__falco_exports.{} = {};\n",
                var_name, rest.trim_start_matches(var_name).trim_start_matches('=').trim(),
                var_name, var_name));
            continue;
        }

        // Handle: export function foo(...) { ... }
        if let Some(rest) = trimmed.strip_prefix("export function ") {
            has_exports = true;
            // Just remove "export " prefix and add to exports.
            let func_name = rest.split('(').next().unwrap_or("").trim();
            output.push_str(&format!("function {}\n__falco_exports.{} = {};\n", rest, func_name, func_name));
            continue;
        }

        // Handle: export default ...
        if let Some(rest) = trimmed.strip_prefix("export default ") {
            has_exports = true;
            output.push_str(&format!("__falco_exports.default = {};\n", rest));
            continue;
        }

        // Handle: export { foo, bar }
        if trimmed.starts_with("export {") && trimmed.ends_with("}") {
            has_exports = true;
            let names_str = trimmed
                .strip_prefix("export {")
                .and_then(|s| s.strip_suffix("}"))
                .unwrap_or("");
            for name in names_str.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    output.push_str(&format!("__falco_exports.{} = {};\n", name, name));
                }
            }
            continue;
        }

        // Regular line — pass through.
        output.push_str(line);
        output.push('\n');
    }

    // Register the exports.
    if has_exports {
        output.push_str("__falco_register(__falco_exports);\n");
    }

    output
}

/// Extract the module path from an import statement.
fn extract_module_path(line: &str) -> String {
    // Find the quoted string after "from".
    if let Some(from_pos) = line.find(" from ") {
        let rest = &line[from_pos + 6..];
        let rest = rest.trim();
        if rest.starts_with('"') {
            return rest.trim_start_matches('"').split('"').next().unwrap_or("").to_string();
        }
        if rest.starts_with('\'') {
            return rest.trim_start_matches('\'').split('\'').next().unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Quote a string for use in JS.
fn quote_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Register the module runtime helpers (__falco_require, __falco_register)
/// on the given scope.
pub fn register_module_runtime(scope: &mut Scope) {
    // __falco_require(path) — loads a module and returns its namespace.
    scope.declare(
        "__falco_require",
        Value::Builtin(BuiltinFn {
            name: "__falco_require".to_string(),
            func: Rc::new(|args| {
                let path = args.first().map(|v| v.to_string()).unwrap_or_default();
                match get_module(&path) {
                    Some(ns) => Ok(ns),
                    None => {
                        // Module not found — return empty object.
                        eprintln!("[falco:modules] module not found: {}", path);
                        Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))
                    }
                }
            }),
        }),
    );

    // __falco_register(exports) — registers the current module's exports.
    scope.declare(
        "__falco_register",
        Value::Builtin(BuiltinFn {
            name: "__falco_register".to_string(),
            func: Rc::new(|args| {
                let exports = args.first().cloned().unwrap_or(Value::Undefined);
                // Store as the "default" module.
                register_module("__current_module__", exports);
                Ok(Value::Undefined)
            }),
        }),
    );

    // __falco_exports — the exports object for the current module.
    scope.declare(
        "__falco_exports",
        Value::Object(Rc::new(RefCell::new(ObjectValue::new()))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_named_import() {
        let input = r#"import { foo, bar } from "./mod.js";"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_require"));
        assert!(output.contains("var foo"));
        assert!(output.contains("var bar"));
    }

    #[test]
    fn transform_default_import() {
        let input = r#"import myDefault from "./mod.js";"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_require"));
        assert!(output.contains("default"));
    }

    #[test]
    fn transform_namespace_import() {
        let input = r#"import * as mod from "./mod.js";"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_require"));
        assert!(output.contains("var mod"));
    }

    #[test]
    fn transform_export_const() {
        let input = r#"export const x = 42;"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_exports.x"));
    }

    #[test]
    fn transform_export_function() {
        let input = r#"export function foo() { return 1; }"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_exports.foo"));
    }

    #[test]
    fn transform_export_default() {
        let input = r#"export default class MyClass {}"#;
        let output = transform_module_syntax(input);
        assert!(output.contains("__falco_exports.default"));
    }

    #[test]
    fn extract_path_double_quotes() {
        assert_eq!(extract_module_path(r#"import { foo } from "./mod.js""#), "./mod.js");
    }

    #[test]
    fn extract_path_single_quotes() {
        assert_eq!(extract_module_path(r#"import { foo } from './mod.js'"#), "./mod.js");
    }

    #[test]
    fn module_registry() {
        register_module("test-mod", Value::Number(42.0));
        let result = get_module("test-mod");
        assert_eq!(result, Some(Value::Number(42.0)));
    }

    #[test]
    fn module_not_found() {
        let result = get_module("nonexistent-module");
        assert_eq!(result, None);
    }
}
