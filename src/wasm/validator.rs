//! WASM module validator.
//!
//! Performs basic structural validation on a parsed `Module`:
//!
//! - Function indices reference valid types
//! - Code section count matches function section count
//! - Export/import indices are within bounds
//! - Start function index is valid
//! - Element/data segment indices are valid
//! - Global init expressions reference only imported globals
//!
//! This is NOT a full type-checker (that would require walking every
//! instruction in every function body and tracking the operand stack).
//! It catches the most common malformed-module bugs.

use crate::wasm::parser::{ExportKind, ImportKind, Module};
use crate::wasm::WasmError;

/// Validate a parsed module. Returns `Ok(())` if valid, `Err` with a
/// description of the problem otherwise.
pub fn validate(module: &Module) -> Result<(), WasmError> {
    // Function section count must match code section count.
    if module.function_indices.len() != module.codes.len() {
        return Err(WasmError::Validate(format!(
            "function section has {} entries but code section has {}",
            module.function_indices.len(),
            module.codes.len()
        )));
    }

    // Each function index must reference a valid type.
    for (i, &type_idx) in module.function_indices.iter().enumerate() {
        if type_idx as usize >= module.types.len() {
            return Err(WasmError::Validate(format!(
                "function {} references type {} but only {} types exist",
                i,
                type_idx,
                module.types.len()
            )));
        }
    }

    // Count imported functions, tables, memories, globals.
    let n_imported_funcs = module
        .imports
        .iter()
        .filter(|i| matches!(i.kind, ImportKind::Function { .. }))
        .count();
    let n_imported_tables = module
        .imports
        .iter()
        .filter(|i| matches!(i.kind, ImportKind::Table { .. }))
        .count();
    let n_imported_memories = module
        .imports
        .iter()
        .filter(|i| matches!(i.kind, ImportKind::Memory { .. }))
        .count();
    let n_imported_globals = module
        .imports
        .iter()
        .filter(|i| matches!(i.kind, ImportKind::Global { .. }))
        .count();

    let n_total_funcs = n_imported_funcs + module.codes.len();
    let n_total_tables = n_imported_tables + module.tables.len();
    let n_total_memories = n_imported_memories + module.memories.len();
    let n_total_globals = n_imported_globals + module.globals.len();

    // Validate imports — function imports must reference valid types.
    for (i, imp) in module.imports.iter().enumerate() {
        if let ImportKind::Function { type_idx } = &imp.kind {
            if *type_idx as usize >= module.types.len() {
                return Err(WasmError::Validate(format!(
                    "import {} ({}.{}) references type {} but only {} types exist",
                    i, imp.module, imp.field, type_idx, module.types.len()
                )));
            }
        }
    }

    // Validate exports — indices must be in bounds.
    for exp in &module.exports {
        match exp.kind {
            ExportKind::Function => {
                if exp.index as usize >= n_total_funcs {
                    return Err(WasmError::Validate(format!(
                        "export \"{}\" references function {} but only {} exist",
                        exp.name, exp.index, n_total_funcs
                    )));
                }
            }
            ExportKind::Table => {
                if exp.index as usize >= n_total_tables {
                    return Err(WasmError::Validate(format!(
                        "export \"{}\" references table {} but only {} exist",
                        exp.name, exp.index, n_total_tables
                    )));
                }
            }
            ExportKind::Memory => {
                if exp.index as usize >= n_total_memories {
                    return Err(WasmError::Validate(format!(
                        "export \"{}\" references memory {} but only {} exist",
                        exp.name, exp.index, n_total_memories
                    )));
                }
            }
            ExportKind::Global => {
                if exp.index as usize >= n_total_globals {
                    return Err(WasmError::Validate(format!(
                        "export \"{}\" references global {} but only {} exist",
                        exp.name, exp.index, n_total_globals
                    )));
                }
            }
        }
    }

    // Validate start function — must be a valid function index, and its
    // signature must be () -> ().
    if let Some(start_idx) = module.start_func {
        if start_idx as usize >= n_total_funcs {
            return Err(WasmError::Validate(format!(
                "start function index {} out of bounds (only {} functions)",
                start_idx, n_total_funcs
            )));
        }
        // Get the function's type.
        let type_idx = if (start_idx as usize) < n_imported_funcs {
            // It's an imported function.
            let mut count = 0;
            let mut found_type = None;
            for imp in &module.imports {
                if let ImportKind::Function { type_idx } = &imp.kind {
                    if count == start_idx as usize {
                        found_type = Some(*type_idx);
                        break;
                    }
                    count += 1;
                }
            }
            found_type
                .ok_or_else(|| WasmError::Validate("start function not found in imports".to_string()))?
        } else {
            // It's a defined function.
            let local_idx = start_idx as usize - n_imported_funcs;
            module.function_indices[local_idx]
        };
        let fty = &module.types[type_idx as usize];
        if !fty.params.is_empty() || !fty.results.is_empty() {
            return Err(WasmError::Validate(format!(
                "start function must have signature () -> (), got ({:?}) -> ({:?})",
                fty.params, fty.results
            )));
        }
    }

    // Validate element segments — table indices must be valid.
    for elem in &module.elements {
        if elem.table_idx as usize >= n_total_tables {
            return Err(WasmError::Validate(format!(
                "element segment references table {} but only {} exist",
                elem.table_idx, n_total_tables
            )));
        }
        for &func_idx in &elem.func_indices {
            if func_idx as usize >= n_total_funcs {
                return Err(WasmError::Validate(format!(
                    "element segment references function {} but only {} exist",
                    func_idx, n_total_funcs
                )));
            }
        }
    }

    // Validate data segments — memory indices must be valid.
    for data in &module.datas {
        if data.memory_idx as usize >= n_total_memories {
            return Err(WasmError::Validate(format!(
                "data segment references memory {} but only {} exist",
                data.memory_idx, n_total_memories
            )));
        }
    }

    // Validate global init expressions — they can only reference imported
    // globals (not locally-defined ones, since those are defined later).
    for (i, global) in module.globals.iter().enumerate() {
        if let Err(e) = validate_init_expr(&global.init_expr, n_imported_globals) {
            return Err(WasmError::Validate(format!(
                "global {} init expression invalid: {}",
                i, e
            )));
        }
    }

    // At most one memory and one table (MVP restriction — though we
    // support multi-memory in the parser, the JS API for MVP only allows one).
    // We don't enforce this here to allow forward compatibility.

    Ok(())
}

/// Validate a constant init expression.
///
/// Init expressions can only contain: const instructions, global.get
/// (on imported globals only), and end.
fn validate_init_expr(bytes: &[u8], n_imported_globals: usize) -> Result<(), String> {
    let mut pos = 0;
    while pos < bytes.len() {
        let op = bytes[pos];
        pos += 1;
        match op {
            0x0B => {
                // end
                return Ok(());
            }
            0x41 => {
                // i32.const — sleb128
                let (_, n) = crate::wasm::parser::decode_sleb128(bytes, pos)
                    .map_err(|e| e.to_string())?;
                pos += n;
            }
            0x42 => {
                // i64.const — sleb128
                let (_, n) = crate::wasm::parser::decode_sleb128(bytes, pos)
                    .map_err(|e| e.to_string())?;
                pos += n;
            }
            0x43 => {
                // f32.const — 4 bytes
                pos += 4;
            }
            0x44 => {
                // f64.const — 8 bytes
                pos += 8;
            }
            0x23 => {
                // global.get — u32
                let (idx, n) = crate::wasm::parser::decode_uleb128(bytes, pos)
                    .map_err(|e| e.to_string())?;
                pos += n;
                if idx as usize >= n_imported_globals {
                    return Err(format!(
                        "init expression references non-imported global {}",
                        idx
                    ));
                }
            }
            _ => {
                return Err(format!("invalid opcode 0x{:02x} in init expression", op));
            }
        }
    }
    Err("init expression missing end opcode".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::parser::*;

    #[test]
    fn validate_empty_module() {
        let module = Module::default();
        assert!(validate(&module).is_ok());
    }

    #[test]
    fn validate_function_count_mismatch() {
        let mut module = Module::default();
        module.function_indices.push(0); // function section says 1
                                          // code section has 0
        assert!(validate(&module).is_err());
    }

    #[test]
    fn validate_invalid_type_index() {
        let mut module = Module::default();
        module.function_indices.push(99); // type 99 doesn't exist
        module.codes.push(FunctionBody {
            locals: vec![],
            code: vec![0x0B], // end
        });
        assert!(validate(&module).is_err());
    }

    #[test]
    fn validate_valid_function() {
        let mut module = Module::default();
        module.types.push(FuncType {
            params: vec![],
            results: vec![],
        });
        module.function_indices.push(0);
        module.codes.push(FunctionBody {
            locals: vec![],
            code: vec![0x0B], // end
        });
        assert!(validate(&module).is_ok());
    }

    #[test]
    fn validate_export_out_of_bounds() {
        let mut module = Module::default();
        module.exports.push(Export {
            name: "f".to_string(),
            kind: ExportKind::Function,
            index: 99, // only 0 functions exist
        });
        assert!(validate(&module).is_err());
    }

    #[test]
    fn validate_start_function_wrong_signature() {
        let mut module = Module::default();
        module.types.push(FuncType {
            params: vec![crate::wasm::value::ValType::I32], // wrong: takes a param
            results: vec![],
        });
        module.function_indices.push(0);
        module.codes.push(FunctionBody {
            locals: vec![],
            code: vec![0x0B],
        });
        module.start_func = Some(0);
        let result = validate(&module);
        assert!(result.is_err());
    }

    #[test]
    fn validate_start_function_correct_signature() {
        let mut module = Module::default();
        module.types.push(FuncType {
            params: vec![],
            results: vec![],
        });
        module.function_indices.push(0);
        module.codes.push(FunctionBody {
            locals: vec![],
            code: vec![0x0B],
        });
        module.start_func = Some(0);
        assert!(validate(&module).is_ok());
    }
}
