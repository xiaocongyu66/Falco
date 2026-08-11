//! WASM binary format parser.
//!
//! Decodes a `.wasm` binary into a structured `Module`. Implements:
//! - LEB128 unsigned and signed integer decoding
//! - Section parsing (all 12 sections per the MVP spec)
//! - Type section (function signatures)
//! - Import section (functions, tables, memories, globals)
//! - Function section (function index → type index mapping)
//! - Table section (function tables)
//! - Memory section (linear memories)
//! - Global section (mutable / immutable globals)
//! - Export section (exported functions/memories/tables/globals)
//! - Start section (start function index)
//! - Element section (table initializers)
//! - Code section (function bodies)
//! - Data section (memory initializers)
//! - Custom section (name section, etc.)
//!
//! Reference: <https://webassembly.github.io/spec/core/binary/>

use crate::wasm::value::ValType;
use crate::wasm::{WasmError, WASM_MAGIC, WASM_VERSION};

// ── LEB128 decoding ────────────────────────────────────────────────────

/// Decode an unsigned LEB128 integer.
///
/// Returns the decoded value and the number of bytes consumed.
pub fn decode_uleb128(bytes: &[u8], offset: usize) -> Result<(u64, usize), WasmError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut i = offset;
    let mut byte;
    loop {
        if i >= bytes.len() {
            return Err(WasmError::Parse("uleb128: unexpected EOF".to_string()));
        }
        byte = bytes[i];
        i += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 63 {
            return Err(WasmError::Parse("uleb128: too many bytes".to_string()));
        }
    }
    Ok((result, i - offset))
}

/// Decode a signed LEB128 integer.
pub fn decode_sleb128(bytes: &[u8], offset: usize) -> Result<(i64, usize), WasmError> {
    let mut result: i64 = 0;
    let mut shift: i32 = 0;
    let mut i = offset;
    let mut byte;
    loop {
        if i >= bytes.len() {
            return Err(WasmError::Parse("sleb128: unexpected EOF".to_string()));
        }
        byte = bytes[i];
        i += 1;
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            // Sign-extend if the high bit of the last byte is set.
            if (byte & 0x40) != 0 && shift < 64 {
                result |= -1_i64 << shift;
            }
            break;
        }
        if shift > 63 {
            return Err(WasmError::Parse("sleb128: too many bytes".to_string()));
        }
    }
    Ok((result, i - offset))
}

/// Decode a 32-bit unsigned LEB128 (used for indices).
pub fn decode_u32_leb128(bytes: &[u8], offset: usize) -> Result<(u32, usize), WasmError> {
    let (v, n) = decode_uleb128(bytes, offset)?;
    Ok((v as u32, n))
}

// ── WASM types ────────────────────────────────────────────────────────

/// A function signature: parameter types and return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// A function limit (for tables and memories).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

/// An imported entity (function, table, memory, or global).
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub field: String,
    pub kind: ImportKind,
}

/// What kind of import this is.
#[derive(Debug, Clone)]
pub enum ImportKind {
    Function { type_idx: u32 },
    Table { element_type: ValType, limits: Limits },
    Memory { limits: Limits },
    Global { val_type: ValType, mutable: bool },
}

/// An exported entity.
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

/// What kind of export this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Function,
    Table,
    Memory,
    Global,
}

/// A global variable definition.
#[derive(Debug, Clone)]
pub struct Global {
    pub val_type: ValType,
    pub mutable: bool,
    pub init_expr: Vec<u8>, // raw bytes of the init expression (const + end)
}

/// A table definition.
#[derive(Debug, Clone)]
pub struct Table {
    pub element_type: ValType, // always funcref (0x70) in MVP
    pub limits: Limits,
}

/// A memory definition.
#[derive(Debug, Clone)]
pub struct Memory {
    pub limits: Limits,
}

/// An element segment (initializes a function table).
#[derive(Debug, Clone)]
pub struct ElementSegment {
    pub table_idx: u32,
    pub offset_expr: Vec<u8>,
    pub func_indices: Vec<u32>,
}

/// A data segment (initializes linear memory).
#[derive(Debug, Clone)]
pub struct DataSegment {
    pub memory_idx: u32,
    pub offset_expr: Vec<u8>,
    pub data: Vec<u8>,
}

/// A function body from the code section.
#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub locals: Vec<(u32, ValType)>, // (count, type) pairs
    pub code: Vec<u8>,               // raw instruction bytes
}

/// A parsed WASM module — ready for validation and instantiation.
#[derive(Debug, Clone, Default)]
pub struct Module {
    /// Function signatures (from the type section).
    pub types: Vec<FuncType>,
    /// Imports (from the import section).
    pub imports: Vec<Import>,
    /// Function index → type index mapping (from the function section).
    pub function_indices: Vec<u32>,
    /// Tables (from the table section).
    pub tables: Vec<Table>,
    /// Memories (from the memory section).
    pub memories: Vec<Memory>,
    /// Globals (from the global section).
    pub globals: Vec<Global>,
    /// Exports (from the export section).
    pub exports: Vec<Export>,
    /// Start function index (from the start section), if any.
    pub start_func: Option<u32>,
    /// Element segments (table initializers).
    pub elements: Vec<ElementSegment>,
    /// Function bodies (from the code section).
    pub codes: Vec<FunctionBody>,
    /// Data segments (memory initializers).
    pub datas: Vec<DataSegment>,
    /// Custom sections (name section, etc.) — stored for debugging.
    pub custom_sections: Vec<(String, Vec<u8>)>,
}

impl Module {
    /// Count the total number of functions (imports + defined).
    pub fn num_functions(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.kind, ImportKind::Function { .. }))
            .count()
            + self.codes.len()
    }

    /// Count the total number of globals (imports + defined).
    pub fn num_globals(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.kind, ImportKind::Global { .. }))
            .count()
            + self.globals.len()
    }

    /// Count the total number of tables (imports + defined).
    pub fn num_tables(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.kind, ImportKind::Table { .. }))
            .count()
            + self.tables.len()
    }

    /// Count the total number of memories (imports + defined).
    pub fn num_memories(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.kind, ImportKind::Memory { .. }))
            .count()
            + self.memories.len()
    }

    /// Find an export by name, returning its kind and index.
    pub fn find_export(&self, name: &str) -> Option<(ExportKind, u32)> {
        self.exports
            .iter()
            .find(|e| e.name == name)
            .map(|e| (e.kind, e.index))
    }
}

// ── Parser ────────────────────────────────────────────────────────────

/// A cursor over the binary, with helper methods for reading.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn read_byte(&mut self) -> Result<u8, WasmError> {
        if self.pos >= self.bytes.len() {
            return Err(WasmError::Parse("unexpected EOF reading byte".to_string()));
        }
        let b = self.bytes[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], WasmError> {
        if self.pos + n > self.bytes.len() {
            return Err(WasmError::Parse(format!(
                "unexpected EOF reading {} bytes at pos {}",
                n, self.pos
            )));
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, WasmError> {
        let (v, n) = decode_u32_leb128(self.bytes, self.pos)?;
        self.pos += n;
        Ok(v)
    }

    fn read_u64(&mut self) -> Result<u64, WasmError> {
        let (v, n) = decode_uleb128(self.bytes, self.pos)?;
        self.pos += n;
        Ok(v)
    }

    fn read_i32(&mut self) -> Result<i32, WasmError> {
        let (v, n) = decode_sleb128(self.bytes, self.pos)?;
        self.pos += n;
        Ok(v as i32)
    }

    fn read_i64(&mut self) -> Result<i64, WasmError> {
        let (v, n) = decode_sleb128(self.bytes, self.pos)?;
        self.pos += n;
        Ok(v)
    }

    fn read_f32(&mut self) -> Result<f32, WasmError> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_f64(&mut self) -> Result<f64, WasmError> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_string(&mut self) -> Result<String, WasmError> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| WasmError::Parse(format!("invalid UTF-8 string: {}", e)))
    }

    fn read_val_type(&mut self) -> Result<ValType, WasmError> {
        let b = self.read_byte()?;
        ValType::from_byte(b)
            .ok_or_else(|| WasmError::Parse(format!("invalid value type byte: 0x{:02x}", b)))
    }

    fn read_limits(&mut self) -> Result<Limits, WasmError> {
        let flag = self.read_byte()?;
        let min = self.read_u32()?;
        let max = if flag == 1 {
            Some(self.read_u32()?)
        } else {
            None
        };
        Ok(Limits { min, max })
    }

    fn read_block_type(&mut self) -> Result<BlockType, WasmError> {
        // A block type is either:
        //   0x40 (empty)
        //   a single ValType byte
        //   or a signed LEB128 type index (for multi-value, post-MVP)
        let b = self.bytes[self.pos];
        if b == 0x40 {
            self.pos += 1;
            return Ok(BlockType::Empty);
        }
        if let Some(vt) = ValType::from_byte(b) {
            self.pos += 1;
            return Ok(BlockType::Single(vt));
        }
        // Otherwise, it's a type index (sleb128).
        let (idx, n) = decode_sleb128(self.bytes, self.pos)?;
        self.pos += n;
        Ok(BlockType::TypeIndex(idx as u32))
    }
}

/// A block's result type.
#[derive(Debug, Clone)]
pub enum BlockType {
    Empty,
    Single(ValType),
    TypeIndex(u32),
}

/// Parse a complete WASM module from its binary representation.
pub fn parse_module(bytes: &[u8]) -> Result<Module, WasmError> {
    if bytes.len() < 8 {
        return Err(WasmError::Parse("file too short for WASM header".to_string()));
    }

    // Magic number.
    if bytes[0..4] != WASM_MAGIC {
        return Err(WasmError::Parse(format!(
            "invalid WASM magic: {:02x?}",
            &bytes[0..4]
        )));
    }

    // Version.
    if bytes[4..8] != WASM_VERSION {
        return Err(WasmError::Parse(format!(
            "unsupported WASM version: {:02x?}",
            &bytes[4..8]
        )));
    }

    let mut module = Module::default();
    let mut r = Reader::new(&bytes[8..]);

    // Parse sections in order. Each section: [id:u8][size:u32][payload].
    while r.remaining() > 0 {
        let section_id = r.read_byte()?;
        let section_size = r.read_u32()? as usize;
        let section_start = r.pos;
        let section_end = section_start + section_size;

        if section_end > r.bytes.len() {
            return Err(WasmError::Parse(format!(
                "section {} size {} exceeds remaining bytes",
                section_id, section_size
            )));
        }

        // Parse the section's payload.
        parse_section(section_id, &mut r, &mut module)?;

        // Sanity check: ensure we consumed exactly `section_size` bytes.
        if r.pos != section_end {
            // Skip any trailing bytes (be lenient).
            r.pos = section_end;
        }
    }

    Ok(module)
}

fn parse_section(id: u8, r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    match id {
        0 => parse_custom_section(r, module),
        1 => parse_type_section(r, module),
        2 => parse_import_section(r, module),
        3 => parse_function_section(r, module),
        4 => parse_table_section(r, module),
        5 => parse_memory_section(r, module),
        6 => parse_global_section(r, module),
        7 => parse_export_section(r, module),
        8 => parse_start_section(r, module),
        9 => parse_element_section(r, module),
        10 => parse_code_section(r, module),
        11 => parse_data_section(r, module),
        12 => parse_data_count_section(r, module),
        _ => {
            // Unknown section — skip it (forward compatibility).
            let size = r.remaining();
            let _ = r.read_bytes(size)?;
            Ok(())
        }
    }
}

fn parse_custom_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let name = r.read_string()?;
    let remaining = r.remaining();
    let data = r.read_bytes(remaining)?.to_vec();
    module.custom_sections.push((name, data));
    Ok(())
}

fn parse_type_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        // Function type marker (0x60).
        let marker = r.read_byte()?;
        if marker != 0x60 {
            return Err(WasmError::Parse(format!(
                "expected function type marker 0x60, got 0x{:02x}",
                marker
            )));
        }
        // Parameter types.
        let n_params = r.read_u32()?;
        let mut params = Vec::with_capacity(n_params as usize);
        for _ in 0..n_params {
            params.push(r.read_val_type()?);
        }
        // Result types.
        let n_results = r.read_u32()?;
        let mut results = Vec::with_capacity(n_results as usize);
        for _ in 0..n_results {
            results.push(r.read_val_type()?);
        }
        module.types.push(FuncType { params, results });
    }
    Ok(())
}

fn parse_import_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let module_name = r.read_string()?;
        let field_name = r.read_string()?;
        let kind_byte = r.read_byte()?;
        let kind = match kind_byte {
            0x00 => {
                let type_idx = r.read_u32()?;
                ImportKind::Function { type_idx }
            }
            0x01 => {
                let elem_type = r.read_byte()?;
                // In MVP, element type must be funcref (0x70).
                if elem_type != 0x70 {
                    return Err(WasmError::Parse(format!(
                        "unsupported table element type: 0x{:02x}",
                        elem_type
                    )));
                }
                let limits = r.read_limits()?;
                ImportKind::Table {
                    element_type: ValType::I32, // placeholder; funcref isn't a ValType
                    limits,
                }
            }
            0x02 => {
                let limits = r.read_limits()?;
                ImportKind::Memory { limits }
            }
            0x03 => {
                let val_type = r.read_val_type()?;
                let mutable = r.read_byte()? != 0;
                ImportKind::Global { val_type, mutable }
            }
            _ => {
                return Err(WasmError::Parse(format!(
                    "unknown import kind: 0x{:02x}",
                    kind_byte
                )))
            }
        };
        module.imports.push(Import {
            module: module_name,
            field: field_name,
            kind,
        });
    }
    Ok(())
}

fn parse_function_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let type_idx = r.read_u32()?;
        module.function_indices.push(type_idx);
    }
    Ok(())
}

fn parse_table_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let elem_type = r.read_byte()?;
        if elem_type != 0x70 {
            return Err(WasmError::Parse(format!(
                "unsupported table element type: 0x{:02x}",
                elem_type
            )));
        }
        let limits = r.read_limits()?;
        module.tables.push(Table {
            element_type: ValType::I32, // placeholder
            limits,
        });
    }
    Ok(())
}

fn parse_memory_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let limits = r.read_limits()?;
        module.memories.push(Memory { limits });
    }
    Ok(())
}

fn parse_global_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let val_type = r.read_val_type()?;
        let mutable = r.read_byte()? != 0;
        // The init expression runs until an `end` opcode (0x0B).
        let init_expr = read_init_expr(r)?;
        module.globals.push(Global {
            val_type,
            mutable,
            init_expr,
        });
    }
    Ok(())
}

fn parse_export_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let name = r.read_string()?;
        let kind_byte = r.read_byte()?;
        let index = r.read_u32()?;
        let kind = match kind_byte {
            0x00 => ExportKind::Function,
            0x01 => ExportKind::Table,
            0x02 => ExportKind::Memory,
            0x03 => ExportKind::Global,
            _ => {
                return Err(WasmError::Parse(format!(
                    "unknown export kind: 0x{:02x}",
                    kind_byte
                )))
            }
        };
        module.exports.push(Export { name, kind, index });
    }
    Ok(())
}

fn parse_start_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let idx = r.read_u32()?;
    module.start_func = Some(idx);
    Ok(())
}

fn parse_element_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let table_idx = r.read_u32()?;
        let offset_expr = read_init_expr(r)?;
        let n_funcs = r.read_u32()?;
        let mut func_indices = Vec::with_capacity(n_funcs as usize);
        for _ in 0..n_funcs {
            func_indices.push(r.read_u32()?);
        }
        module.elements.push(ElementSegment {
            table_idx,
            offset_expr,
            func_indices,
        });
    }
    Ok(())
}

fn parse_code_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        // Body size (we read past it but don't strictly need it).
        let _body_size = r.read_u32()? as usize;
        let body_start = r.pos;

        // Local declarations.
        let n_local_decls = r.read_u32()?;
        let mut locals = Vec::with_capacity(n_local_decls as usize);
        for _ in 0..n_local_decls {
            let n = r.read_u32()?;
            let vt = r.read_val_type()?;
            locals.push((n, vt));
        }

        // The rest is the code, terminated by an `end` (0x0B) opcode.
        // We read everything remaining in the body.
        let body_end = body_start + _body_size;
        let code = r.bytes[r.pos..body_end].to_vec();
        r.pos = body_end;

        module.codes.push(FunctionBody { locals, code });
    }
    Ok(())
}

fn parse_data_section(r: &mut Reader, module: &mut Module) -> Result<(), WasmError> {
    let count = r.read_u32()?;
    for _ in 0..count {
        let memory_idx = r.read_u32()?;
        let offset_expr = read_init_expr(r)?;
        let n_bytes = r.read_u32()? as usize;
        let data = r.read_bytes(n_bytes)?.to_vec();
        module.datas.push(DataSegment {
            memory_idx,
            offset_expr,
            data,
        });
    }
    Ok(())
}

fn parse_data_count_section(r: &mut Reader, _module: &mut Module) -> Result<(), WasmError> {
    // Just read and discard the count (used for bulk memory operations).
    let _count = r.read_u32()?;
    Ok(())
}

/// Read an init expression — a sequence of instructions ending with `end` (0x0B).
fn read_init_expr(r: &mut Reader) -> Result<Vec<u8>, WasmError> {
    let start = r.pos;
    // Scan until we find the `end` opcode.
    loop {
        if r.pos >= r.bytes.len() {
            return Err(WasmError::Parse("init expression: unexpected EOF".to_string()));
        }
        let b = r.bytes[r.pos];
        r.pos += 1;
        if b == 0x0B {
            // End opcode.
            break;
        }
        // Skip the instruction's operands. We need to know the operand sizes
        // for const instructions (the only ones that appear in init exprs).
        match b {
            0x41 => {
                // i32.const — sleb128
                let (_, n) = decode_sleb128(r.bytes, r.pos)?;
                r.pos += n;
            }
            0x42 => {
                // i64.const — sleb128
                let (_, n) = decode_sleb128(r.bytes, r.pos)?;
                r.pos += n;
            }
            0x43 => {
                // f32.const — 4 bytes
                r.pos += 4;
            }
            0x44 => {
                // f64.const — 8 bytes
                r.pos += 8;
            }
            0x23 => {
                // global.get — u32
                let (_, n) = decode_uleb128(r.bytes, r.pos)?;
                r.pos += n;
            }
            _ => {
                // Unknown instruction in init expr — keep scanning.
            }
        }
    }
    Ok(r.bytes[start..r.pos].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_small_unsigned() {
        // 0 encoded as single byte 0x00.
        let (v, n) = decode_uleb128(&[0x00], 0).unwrap();
        assert_eq!(v, 0);
        assert_eq!(n, 1);

        // 1 encoded as 0x01.
        let (v, n) = decode_uleb128(&[0x01], 0).unwrap();
        assert_eq!(v, 1);
        assert_eq!(n, 1);

        // 127 encoded as 0x7F.
        let (v, n) = decode_uleb128(&[0x7F], 0).unwrap();
        assert_eq!(v, 127);
        assert_eq!(n, 1);
    }

    #[test]
    fn leb128_multi_byte_unsigned() {
        // 128 = 0x80 0x01
        let (v, n) = decode_uleb128(&[0x80, 0x01], 0).unwrap();
        assert_eq!(v, 128);
        assert_eq!(n, 2);

        // 300 = 0xAC 0x02
        let (v, n) = decode_uleb128(&[0xAC, 0x02], 0).unwrap();
        assert_eq!(v, 300);
        assert_eq!(n, 2);
    }

    #[test]
    fn leb128_signed() {
        // 0 = 0x00
        let (v, _) = decode_sleb128(&[0x00], 0).unwrap();
        assert_eq!(v, 0);

        // 1 = 0x01
        let (v, _) = decode_sleb128(&[0x01], 0).unwrap();
        assert_eq!(v, 1);

        // -1 = 0x7F
        let (v, _) = decode_sleb128(&[0x7F], 0).unwrap();
        assert_eq!(v, -1);

        // 63 = 0x3F
        let (v, _) = decode_sleb128(&[0x3F], 0).unwrap();
        assert_eq!(v, 63);

        // -64 = 0x40
        let (v, _) = decode_sleb128(&[0x40], 0).unwrap();
        assert_eq!(v, -64);

        // -12345 = 0xC7 0x9F 0x7F
        let (v, _) = decode_sleb128(&[0xC7, 0x9F, 0x7F], 0).unwrap();
        assert_eq!(v, -12345);
    }

    #[test]
    fn parse_minimal_module() {
        // A minimal valid WASM module: just the header.
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        let module = parse_module(&bytes).unwrap();
        assert!(module.types.is_empty());
        assert!(module.function_indices.is_empty());
    }

    #[test]
    fn parse_module_with_function_type() {
        // Header + type section with one signature: (i32) -> (i32)
        let bytes = vec![
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // header
            0x01, // section id: type
            0x06, // section size: 6 bytes
            0x01, // count: 1 type
            0x60, // func type marker
            0x01, // 1 param
            0x7F, // i32
            0x01, // 1 result
            0x7F, // i32
        ];
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.types[0].params, vec![ValType::I32]);
        assert_eq!(module.types[0].results, vec![ValType::I32]);
    }

    #[test]
    fn parse_module_with_export() {
        let bytes = vec![
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // header
            // Export section: export "add" as function 0.
            0x07, // section id: export
            0x07, // section size
            0x01, // count: 1
            0x03, // name length: 3
            b'a', b'd', b'd', // "add"
            0x00, // kind: function
            0x00, // index: 0
        ];
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.exports.len(), 1);
        assert_eq!(module.exports[0].name, "add");
        assert_eq!(module.exports[0].kind, ExportKind::Function);
        assert_eq!(module.exports[0].index, 0);
    }

    #[test]
    fn parse_module_with_memory() {
        let bytes = vec![
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // header
            // Memory section: 1 memory, min=1 page, no max.
            0x05, // section id: memory
            0x03, // section size
            0x01, // count: 1
            0x00, // limits flag: no max
            0x01, // min: 1 page
        ];
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.memories.len(), 1);
        assert_eq!(module.memories[0].limits.min, 1);
        assert_eq!(module.memories[0].limits.max, None);
    }

    #[test]
    fn invalid_magic_rejected() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x00, 0x00, 0x00];
        assert!(parse_module(&bytes).is_err());
    }

    #[test]
    fn invalid_version_rejected() {
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00];
        assert!(parse_module(&bytes).is_err());
    }
}
