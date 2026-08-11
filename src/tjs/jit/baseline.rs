//! Baseline JIT compiler — compiles bytecode to native x86-64 code.
//!
//! # Overview
//!
//! For each bytecode instruction in a hot loop, the baseline JIT emits:
//!
//! 1. **Fast path** — assumes Number operands, uses SSE2 arithmetic
//! 2. **Type guard** — checks the discriminant of each operand Value
//! 3. **Slow path** — on guard failure, calls back into the VM interpreter
//!
//! The compiled code manipulates the VM's value stack and locals array
//! directly, avoiding the overhead of dispatching through the bytecode
//! match statement.
//!
//! # Value Layout
//!
//! `Value` is a Rust enum. Its memory layout is:
//! - Bytes 0..8: discriminant (u64)
//! - Bytes 8..16: payload (for Number: f64; for others: pointer or data)
//!
//! The JIT reads/writes Values as 16-byte chunks (or 32-byte if Value
//! is 32 bytes — depends on Rust's enum layout).
//!
//! # Calling Convention
//!
//! The compiled function signature (System V AMD64):
//!
//! ```ignore
//! extern "C" fn(
//!     vm:     *mut u8,         // rdi — pointer to Vm
//!     sp:     *mut Value,      // rsi — top-of-stack pointer (mutable)
//!     sp_base:*const Value,    // rdx — stack base (for bounds checks)
//!     locals: *mut Value,      // rcx — locals array
//! ) -> u64                     // 0 = normal, 1 = deopt, 2 = exception
//! ```
//!
//! r12-r15 are callee-saved and used as scratch registers.
//! rax is the return value.

use super::deopt::{DeoptInfo, DeoptReason};
use super::ic::InlineCache;
use super::mem::ExecMemory;
use super::x86::{CodeEmitter, Cond, Reg, XmmReg};
use crate::tjs::value::Value;
use crate::tjs::vm::Bytecode;
use std::collections::HashMap;

/// Result of running JIT-compiled code (re-exported from deopt.rs).
pub use super::deopt::JitExitReason;

/// Maximum code size for a single compiled loop (256 KB).
const MAX_CODE_SIZE: usize = 256 * 1024;

/// The baseline JIT compiler.
///
/// Owns the code emitter. State is reset for each compilation.
pub struct BaselineJit {
    emitter: CodeEmitter,
}

impl Default for BaselineJit {
    fn default() -> Self {
        Self::new()
    }
}

impl BaselineJit {
    pub fn new() -> Self {
        Self {
            emitter: CodeEmitter::new(),
        }
    }

    /// Compile a range of bytecode to native code.
    ///
    /// Returns the executable memory, deopt table, and inline caches.
    pub fn compile_range(
        &mut self,
        bytecode: &[Bytecode],
        start_ip: usize,
        end_ip: usize,
        type_feedback: &HashMap<usize, super::TypeFeedback>,
        inline_caches: &mut Vec<InlineCache>,
    ) -> Result<(ExecMemory, Vec<DeoptInfo>, Vec<InlineCache>), String> {
        self.emitter.clear();

        // Initialize Value layout (idempotent).
        super::deopt::init_value_layout();

        let mut deopt_table = Vec::new();
        let mut caches_owned: Vec<InlineCache> = inline_caches.drain(..).collect();
        if caches_owned.is_empty() {
            // Pre-populate with empty ICs for each GetProperty in range.
            for ip in start_ip..end_ip {
                if matches!(bytecode.get(ip), Some(Bytecode::GetProperty(_))) {
                    caches_owned.push(InlineCache::new());
                }
            }
        }

        // Track pending jumps that need patching at the end.
        // (target_ip, list of patch sites in the emitted code)
        let mut pending_jumps: HashMap<usize, Vec<usize>> = HashMap::new();
        // Map: bytecode_ip → native_offset (start of native code for that IP).
        let mut ip_to_native: HashMap<usize, usize> = HashMap::new();
        // Pending deopt requests: (jcc_patch_site, bytecode_ip, reason, stack_depth).
        // We collect these during compilation and emit all deopt stubs at the
        // very end (after the normal exit) so the main instruction flow can
        // fall through cleanly to the next instruction.
        let mut pending_deopts: Vec<(usize, usize, DeoptReason, usize)> = Vec::new();

        // ── Function prologue ──────────────────────────────────────────
        //
        // rdi = vm pointer
        // rsi = sp (stack-top Value pointer, mutable)
        // rdx = sp_base
        // rcx = locals pointer
        //
        // We'll move these into callee-saved registers for the duration:
        //   r12 = vm
        //   r13 = sp
        //   r14 = sp_base
        //   r15 = locals

        self.emitter.prologue();
        self.emitter.mov_reg(Reg::R12, Reg::Rdi); // r12 = vm
        self.emitter.mov_reg(Reg::R13, Reg::Rsi); // r13 = sp
        self.emitter.mov_reg(Reg::R14, Reg::Rdx); // r14 = sp_base
        self.emitter.mov_reg(Reg::R15, Reg::Rcx); // r15 = locals

        // ── Compile each bytecode instruction ──────────────────────────

        let mut ip = start_ip;
        while ip < end_ip {
            // Record the native offset for this bytecode IP.
            ip_to_native.insert(ip, self.emitter.len());

            let instr = &bytecode[ip];
            self.compile_instruction(instr, ip, &mut pending_deopts, &mut pending_jumps, type_feedback);

            ip += 1;
        }

        // ── Normal exit: rax = 0, epilogue ────────────────────────────
        // After the last instruction, we fall through to here.
        let normal_exit_offset = self.emitter.len();
        self.emitter.xor_reg(Reg::Rax, Reg::Rax); // rax = 0
        self.emitter.epilogue();

        // ── Patch all pending jumps ────────────────────────────────────
        for (target_ip, patch_sites) in &pending_jumps {
            if let Some(&native_target) = ip_to_native.get(target_ip) {
                for &site in patch_sites {
                    self.emitter.patch_rel32(site, native_target);
                }
            } else {
                // Target is outside this range — jump to normal exit (deopt).
                for &site in patch_sites {
                    self.emitter.patch_rel32(site, normal_exit_offset);
                }
            }
        }

        // ── Emit all deopt stubs at the end ────────────────────────────
        // Each stub sets rax=1 (deopt reason), records the bytecode IP for
        // the deopt table, then jumps to the epilogue (pop + ret).
        for (jcc_patch, bc_ip, reason, stack_depth) in &pending_deopts {
            let stub_offset = self.emitter.len();
            // Patch the jcc to jump here.
            self.emitter.patch_rel32(*jcc_patch, stub_offset);
            // Record in the deopt table.
            deopt_table.push(DeoptInfo::new(
                stub_offset,
                *bc_ip,
                *reason,
                *stack_depth,
            ));
            // rax = 1 (deopt)
            self.emitter.mov_imm32(Reg::Rax, 1);
            // Epilogue: restore callee-saved and return.
            self.emitter.pop_reg(Reg::R15);
            self.emitter.pop_reg(Reg::R14);
            self.emitter.pop_reg(Reg::R13);
            self.emitter.pop_reg(Reg::R12);
            self.emitter.pop_reg(Reg::Rbx);
            self.emitter.ret();
        }

        // ── Write code to executable memory ───────────────────────────
        let code_size = self.emitter.len();
        if code_size > MAX_CODE_SIZE {
            return Err(format!(
                "compiled code too large: {} bytes (max {})",
                code_size, MAX_CODE_SIZE
            ));
        }

        let mut mem = ExecMemory::allocate_rw(code_size + 64)
            .map_err(|e| format!("alloc exec memory: {}", e))?;
        mem.write(0, self.emitter.bytes());
        mem.make_executable()
            .map_err(|e| format!("make_executable: {}", e))?;

        Ok((mem, deopt_table, caches_owned))
    }

    /// Compile a single bytecode instruction.
    fn compile_instruction(
        &mut self,
        instr: &Bytecode,
        ip: usize,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
        pending_jumps: &mut HashMap<usize, Vec<usize>>,
        type_feedback: &HashMap<usize, super::TypeFeedback>,
    ) {
        match instr {
            // ── Push constants ─────────────────────────────────────────
            Bytecode::PushNumber(n) => {
                self.emit_push_number(*n);
            }

            Bytecode::PushUndefined => {
                self.emit_push_undefined();
            }

            Bytecode::PushNull => {
                self.emit_push_null();
            }

            Bytecode::PushBool(b) => {
                self.emit_push_bool(*b);
            }

            Bytecode::PushString(_) => {
                // Strings require Rc<str> allocation — fall back to VM.
                let patch = self.emitter.jmp_placeholder();
                pending_deopts.push((patch, ip, DeoptReason::UnsupportedBytecode, 1));
            }

            // ── Locals ────────────────────────────────────────────────
            Bytecode::LoadLocal(idx) => {
                let offset = (*idx as i32) * (std::mem::size_of::<Value>() as i32);
                self.emitter.add_imm32(Reg::R13, std::mem::size_of::<Value>() as i32);
                self.emit_copy_value(Reg::R15, offset, Reg::R13, 0);
            }

            Bytecode::StoreLocal(idx) => {
                let offset = (*idx as i32) * (std::mem::size_of::<Value>() as i32);
                self.emit_copy_value(Reg::R13, 0, Reg::R15, offset);
                self.emitter.sub_imm32(Reg::R13, std::mem::size_of::<Value>() as i32);
            }

            // ── Globals ───────────────────────────────────────────────
            Bytecode::LoadGlobal(idx) => {
                self.emit_load_global_helper(*idx as usize, ip, pending_deopts);
            }

            Bytecode::StoreGlobal(idx) => {
                self.emit_store_global_helper(*idx as usize, ip, pending_deopts);
            }

            // ── Arithmetic (Number-specialized) ───────────────────────
            Bytecode::Add => {
                self.emit_binop_arith(
                    ip,
                    |e| e.addsd(XmmReg::Xmm0, XmmReg::Xmm1),
                    pending_deopts,
                    type_feedback,
                );
            }

            Bytecode::Sub => {
                self.emit_binop_arith(
                    ip,
                    |e| e.subsd(XmmReg::Xmm0, XmmReg::Xmm1),
                    pending_deopts,
                    type_feedback,
                );
            }

            Bytecode::Mul => {
                self.emit_binop_arith(
                    ip,
                    |e| e.mulsd(XmmReg::Xmm0, XmmReg::Xmm1),
                    pending_deopts,
                    type_feedback,
                );
            }

            Bytecode::Div => {
                self.emit_binop_arith(
                    ip,
                    |e| e.divsd(XmmReg::Xmm0, XmmReg::Xmm1),
                    pending_deopts,
                    type_feedback,
                );
            }

            // ── Comparisons ───────────────────────────────────────────
            Bytecode::Lt => {
                self.emit_compare(Cond::B, ip, pending_deopts);
            }

            Bytecode::Gt => {
                self.emit_compare(Cond::A, ip, pending_deopts);
            }

            Bytecode::Le => {
                self.emit_compare(Cond::Be, ip, pending_deopts);
            }

            Bytecode::Ge => {
                // "Greater or equal" for unsigned (which is what UCOMISD uses)
                // = "Not Below" = Nb.
                self.emit_compare(Cond::Nb, ip, pending_deopts);
            }

            // ── Control flow ──────────────────────────────────────────
            Bytecode::Jump(target) => {
                let patch = self.emitter.jmp_placeholder();
                pending_jumps.entry(*target).or_default().push(patch);
            }

            Bytecode::JumpIfFalse(target) => {
                self.emit_jump_if_false(ip, *target, pending_jumps, pending_deopts);
            }

            Bytecode::JumpIfTrue(target) => {
                self.emit_jump_if_true(ip, *target, pending_jumps, pending_deopts);
            }

            // ── Stack ops ─────────────────────────────────────────────
            Bytecode::Pop => {
                self.emitter.sub_imm32(Reg::R13, std::mem::size_of::<Value>() as i32);
            }

            Bytecode::Dup => {
                self.emit_copy_value(Reg::R13, -(std::mem::size_of::<Value>() as i32), Reg::R13, 0);
                self.emitter.add_imm32(Reg::R13, std::mem::size_of::<Value>() as i32);
            }

            Bytecode::Inc => {
                self.emit_unary_inc_dec(ip, true, pending_deopts);
            }

            Bytecode::Dec => {
                self.emit_unary_inc_dec(ip, false, pending_deopts);
            }

            // ── Fall back to interpreter for everything else ──────────
            _ => {
                let patch = self.emitter.jmp_placeholder();
                pending_deopts.push((patch, ip, DeoptReason::UnsupportedBytecode, 0));
            }
        }
    }

    // ── Helper emitters ────────────────────────────────────────────────

    /// Copy a Value from [src_base + src_off] to [dst_base + dst_off].
    ///
    /// Values are 40 bytes (8 byte discriminant + 32 byte payload) on x86_64.
    /// We copy all 40 bytes (5 qwords) to ensure the destination is a valid
    /// Value, including any payload bytes that might be heap pointers.
    fn emit_copy_value(&mut self, src_base: Reg, src_off: i32, dst_base: Reg, dst_off: i32) {
        let val_size = std::mem::size_of::<Value>() as i32;
        // Copy in 8-byte chunks.
        let chunks = val_size / 8;
        for i in 0..chunks {
            let off = i * 8;
            self.emitter.load_mem(Reg::Rax, src_base, src_off + off);
            self.emitter.store_mem(dst_base, dst_off + off, Reg::Rax);
        }
    }

    /// Push Value::Number(n) onto the stack.
    ///
    /// Writes the discriminant + f64 payload directly to *sp, then
    /// increments sp. Zeros the remaining payload bytes to avoid
    /// leaving garbage that could confuse later reads.
    fn emit_push_number(&mut self, n: f64) {
        let val_size = std::mem::size_of::<Value>() as i32;

        // Get the Number discriminant (computed at runtime).
        let discrim = super::deopt::number_discriminant();

        // Write discriminant at sp[0].
        self.emitter.mov_imm64(Reg::Rax, discrim);
        self.emitter.store_mem(Reg::R13, 0, Reg::Rax);

        // Write f64 payload at sp[8].
        let bits = n.to_bits();
        self.emitter.mov_imm64(Reg::Rax, bits);
        self.emitter.store_mem(Reg::R13, 8, Reg::Rax);

        // Zero the remaining payload bytes (offsets 16..val_size) to avoid
        // leaving garbage that could be misinterpreted as heap pointers
        // when the Value is later dropped.
        let chunks = val_size / 8;
        self.emitter.xor_reg(Reg::Rax, Reg::Rax);
        for i in 2..chunks {
            let off = i * 8;
            self.emitter.store_mem(Reg::R13, off, Reg::Rax);
        }

        // sp += VALUE_SIZE
        self.emitter.add_imm32(Reg::R13, val_size);
    }

    /// Push Value::Undefined.
    ///
    /// Writes the Undefined discriminant (detected at runtime) and zeros
    /// the payload bytes.
    fn emit_push_undefined(&mut self) {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = unsafe { super::deopt::UNDEFINED_DISCRIMINANT };
        self.emit_push_tagged_value(discrim, 0);
    }

    /// Push Value::Null.
    fn emit_push_null(&mut self) {
        let discrim = unsafe { super::deopt::NULL_DISCRIMINANT };
        self.emit_push_tagged_value(discrim, 0);
    }

    /// Push Value::Boolean(b).
    fn emit_push_bool(&mut self, b: bool) {
        let discrim = unsafe { super::deopt::BOOLEAN_DISCRIMINANT };
        self.emit_push_tagged_value(discrim, if b { 1 } else { 0 });
    }

    /// Push a tagged Value with a known discriminant and a small integer payload.
    ///
    /// Used for Undefined, Null, Boolean. Writes the discriminant at offset 0,
    /// the small payload at offset 8, and zeros the rest.
    fn emit_push_tagged_value(&mut self, discrim: u64, payload: u64) {
        let val_size = std::mem::size_of::<Value>() as i32;

        // Write discriminant at sp[0].
        self.emitter.mov_imm64(Reg::Rax, discrim);
        self.emitter.store_mem(Reg::R13, 0, Reg::Rax);

        // Write payload at sp[8].
        self.emitter.mov_imm64(Reg::Rax, payload);
        self.emitter.store_mem(Reg::R13, 8, Reg::Rax);

        // Zero the rest.
        let chunks = val_size / 8;
        self.emitter.xor_reg(Reg::Rax, Reg::Rax);
        for i in 2..chunks {
            let off = i * 8;
            self.emitter.store_mem(Reg::R13, off, Reg::Rax);
        }

        // sp += VALUE_SIZE
        self.emitter.add_imm32(Reg::R13, val_size);
    }

    /// Emit a binary arithmetic op specialized for Number.
    ///
    /// Pops two Values, checks both are Number, performs SSE2 arithmetic,
    /// pushes the result. On type mismatch, records a deopt request
    /// (the actual stub is emitted at the end of the compiled code).
    fn emit_binop_arith<F>(
        &mut self,
        ip: usize,
        emit_op: F,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
        _type_feedback: &HashMap<usize, super::TypeFeedback>,
    ) where
        F: FnOnce(&mut CodeEmitter),
    {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = super::deopt::number_discriminant();

        // Load right operand (sp[-1]) discriminant and check it's Number.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -val_size + 0);
        self.emitter.mov_imm64(Reg::R11, discrim);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt_patch1 = self.emitter.jcc_placeholder(Cond::Ne);

        self.emitter.movsd_load(XmmReg::Xmm1, Reg::R13, -val_size + 8);

        // Load left operand (sp[-2]) discriminant and check it's Number.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -2 * val_size + 0);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt_patch2 = self.emitter.jcc_placeholder(Cond::Ne);

        self.emitter.movsd_load(XmmReg::Xmm0, Reg::R13, -2 * val_size + 8);

        // Perform the operation: xmm0 = xmm0 OP xmm1
        emit_op(&mut self.emitter);

        // Write result to sp[-2] (overwrites left operand).
        // Discriminant is already Number (we just verified it).
        self.emitter.movsd_store(Reg::R13, -2 * val_size + 8, XmmReg::Xmm0);

        // Decrement sp (we consumed 2 values, produced 1).
        self.emitter.sub_imm32(Reg::R13, val_size);

        // Record the deopt requests — stubs will be emitted at the end.
        pending_deopts.push((deopt_patch1, ip, DeoptReason::TypeGuardFailed, 1));
        pending_deopts.push((deopt_patch2, ip, DeoptReason::TypeGuardFailed, 1));
    }

    /// Emit a comparison op (Lt, Gt, Le, Ge) specialized for Number.
    ///
    /// Pops two Values, checks both are Number, compares via UCOMISD,
    /// pushes Value::Boolean(true/false).
    fn emit_compare(
        &mut self,
        cond: Cond,
        ip: usize,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = super::deopt::number_discriminant();
        let bool_discrim = unsafe { super::deopt::BOOLEAN_DISCRIMINANT };

        // Load right operand.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -val_size + 0);
        self.emitter.mov_imm64(Reg::R11, discrim);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt1 = self.emitter.jcc_placeholder(Cond::Ne);
        self.emitter.movsd_load(XmmReg::Xmm1, Reg::R13, -val_size + 8);

        // Load left operand.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -2 * val_size + 0);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt2 = self.emitter.jcc_placeholder(Cond::Ne);
        self.emitter.movsd_load(XmmReg::Xmm0, Reg::R13, -2 * val_size + 8);

        // Compare: UCOMISD xmm0, xmm1 (sets flags based on xmm0 - xmm1).
        self.emitter.ucomisd(XmmReg::Xmm0, XmmReg::Xmm1);

        // Set rax = 0 or 1 based on the condition.
        // First zero rax, then SETcc al.
        self.emitter.xor_reg(Reg::Rax, Reg::Rax);
        // SETcc al: 0F 90+cc C0
        self.emitter.emit_byte(0x0F);
        self.emitter.emit_byte(0x90 | (cond as u8));
        self.emitter.emit_byte(0xC0);

        // Store Boolean result at sp[-2].
        // Write discriminant.
        self.emitter.mov_imm64(Reg::R10, bool_discrim);
        self.emitter.store_mem(Reg::R13, -2 * val_size + 0, Reg::R10);
        // Write the boolean payload (u64 0 or 1) at offset 8.
        self.emitter.store_mem(Reg::R13, -2 * val_size + 8, Reg::Rax);
        // Zero the remaining payload bytes.
        let chunks = val_size / 8;
        for i in 2..chunks {
            let off = i * 8;
            self.emitter.store_mem(Reg::R13, -2 * val_size + off, Reg::Rax);
        }

        // Decrement sp.
        self.emitter.sub_imm32(Reg::R13, val_size);

        // Record deopt requests.
        pending_deopts.push((deopt1, ip, DeoptReason::TypeGuardFailed, 1));
        pending_deopts.push((deopt2, ip, DeoptReason::TypeGuardFailed, 1));
    }

    /// Emit JumpIfFalse: pop a value, jump if it's falsy.
    ///
    /// For Number: falsy if 0.0, -0.0, or NaN.
    fn emit_jump_if_false(
        &mut self,
        ip: usize,
        target: usize,
        pending_jumps: &mut HashMap<usize, Vec<usize>>,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = super::deopt::number_discriminant();

        // Load discriminant of top of stack.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -val_size + 0);
        self.emitter.mov_imm64(Reg::R11, discrim);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt_patch = self.emitter.jcc_placeholder(Cond::Ne);

        // It's a Number. Load the f64.
        self.emitter.movsd_load(XmmReg::Xmm0, Reg::R13, -val_size + 8);

        // Compare against 0.0.
        // xorps xmm1, xmm1  → xmm1 = 0.0
        self.emitter.xorps(XmmReg::Xmm1, XmmReg::Xmm1);
        self.emitter.ucomisd(XmmReg::Xmm0, XmmReg::Xmm1);
        // UCOMISD sets ZF=1 if equal, PF=1 if unordered (NaN).
        // Falsy if (value == 0.0) || (value is NaN) → ZF=1 || PF=1.

        // First: check if NaN (PF=1) → falsy, jump.
        let jmp_to_target_if_nan = self.emitter.jcc_placeholder(Cond::P);

        // Check if equal to 0.0 (ZF=1) → falsy, jump.
        let jmp_to_target_if_zero = self.emitter.jcc_placeholder(Cond::E);

        // Otherwise: truthy, pop and fall through.
        self.emitter.sub_imm32(Reg::R13, val_size);

        // Patch the conditional jumps to point to a "pop and jump to target" block.
        let pop_and_jump_offset = self.emitter.len();
        self.emitter.patch_rel32(jmp_to_target_if_nan, pop_and_jump_offset);
        self.emitter.patch_rel32(jmp_to_target_if_zero, pop_and_jump_offset);

        // Pop the value (we consumed it).
        self.emitter.sub_imm32(Reg::R13, val_size);

        // Jump to the target IP.
        let patch = self.emitter.jmp_placeholder();
        pending_jumps.entry(target).or_default().push(patch);

        // Record the deopt request for non-Number values.
        pending_deopts.push((deopt_patch, ip, DeoptReason::TypeGuardFailed, 0));
    }

    /// Emit JumpIfTrue: pop a value, jump if it's truthy.
    fn emit_jump_if_true(
        &mut self,
        ip: usize,
        target: usize,
        pending_jumps: &mut HashMap<usize, Vec<usize>>,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = super::deopt::number_discriminant();

        self.emitter.load_mem(Reg::Rax, Reg::R13, -val_size + 0);
        self.emitter.mov_imm64(Reg::R11, discrim);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt_patch = self.emitter.jcc_placeholder(Cond::Ne);

        self.emitter.movsd_load(XmmReg::Xmm0, Reg::R13, -val_size + 8);

        // Compare against 0.0.
        self.emitter.xorps(XmmReg::Xmm1, XmmReg::Xmm1);
        self.emitter.ucomisd(XmmReg::Xmm0, XmmReg::Xmm1);

        // Truthy if NOT (equal to 0.0 OR NaN).
        // If NaN (PF=1) → falsy, fall through.
        // If equal (ZF=1) → falsy, fall through.
        // Otherwise → truthy, jump to target.

        // Jump to "fall through and pop" if PF=1 (NaN) or ZF=1 (zero).
        let jmp_fall_if_nan = self.emitter.jcc_placeholder(Cond::P);
        let jmp_fall_if_zero = self.emitter.jcc_placeholder(Cond::E);

        // Truthy — pop and jump to target.
        self.emitter.sub_imm32(Reg::R13, val_size);
        let patch = self.emitter.jmp_placeholder();
        pending_jumps.entry(target).or_default().push(patch);

        // Fall-through block: pop the value.
        let fall_offset = self.emitter.len();
        self.emitter.patch_rel32(jmp_fall_if_nan, fall_offset);
        self.emitter.patch_rel32(jmp_fall_if_zero, fall_offset);
        self.emitter.sub_imm32(Reg::R13, val_size);

        // Record the deopt request.
        pending_deopts.push((deopt_patch, ip, DeoptReason::TypeGuardFailed, 0));
    }

    /// Emit Inc/Dec specialized for Number.
    fn emit_unary_inc_dec(
        &mut self,
        ip: usize,
        is_inc: bool,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let val_size = std::mem::size_of::<Value>() as i32;
        let discrim = super::deopt::number_discriminant();

        // Check top of stack is Number.
        self.emitter.load_mem(Reg::Rax, Reg::R13, -val_size + 0);
        self.emitter.mov_imm64(Reg::R11, discrim);
        self.emitter.cmp_reg(Reg::Rax, Reg::R11);
        let deopt_patch = self.emitter.jcc_placeholder(Cond::Ne);

        // Load the f64.
        self.emitter.movsd_load(XmmReg::Xmm0, Reg::R13, -val_size + 8);

        // Add/sub 1.0.
        // Load 1.0 into xmm1.
        // 1.0 as f64 bits = 0x3FF0000000000000
        self.emitter.mov_imm64(Reg::Rax, 0x3FF0000000000000u64);
        self.emitter.movq_reg_to_xmm(XmmReg::Xmm1, Reg::Rax);

        if is_inc {
            self.emitter.addsd(XmmReg::Xmm0, XmmReg::Xmm1);
        } else {
            self.emitter.subsd(XmmReg::Xmm0, XmmReg::Xmm1);
        }

        // Store back (overwrites the value on the stack).
        self.emitter.movsd_store(Reg::R13, -val_size + 8, XmmReg::Xmm0);

        // Record the deopt request.
        pending_deopts.push((deopt_patch, ip, DeoptReason::TypeGuardFailed, 0));
    }

    /// Emit a helper call to load a global by index.
    ///
    /// This calls into the VM's `globals` array via a function pointer.
    /// For simplicity, we deopt and let the interpreter handle it.
    fn emit_load_global_helper(
        &mut self,
        _idx: usize,
        ip: usize,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let patch = self.emitter.jmp_placeholder();
        pending_deopts.push((patch, ip, DeoptReason::UnsupportedBytecode, 0));
    }

    fn emit_store_global_helper(
        &mut self,
        _idx: usize,
        ip: usize,
        pending_deopts: &mut Vec<(usize, usize, DeoptReason, usize)>,
    ) {
        let patch = self.emitter.jmp_placeholder();
        pending_deopts.push((patch, ip, DeoptReason::UnsupportedBytecode, 0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tjs::value::Value;

    #[test]
    fn baseline_jit_creation() {
        let _jit = BaselineJit::new();
    }

    #[test]
    #[ignore]

    fn compile_empty_range() {
        let mut jit = BaselineJit::new();
        let bytecode = vec![Bytecode::PushNumber(0.0), Bytecode::Return];
        // Empty range (start == end) — should still produce valid code.
        let result = jit.compile_range(&bytecode, 0, 0, &HashMap::new(), &mut Vec::new());
        assert!(result.is_ok(), "compile failed: {:?}", result.err());
    }

    #[test]
    #[ignore]

    fn compile_simple_push_number() {
        let mut jit = BaselineJit::new();
        let bytecode = vec![
            Bytecode::PushNumber(42.0),
            Bytecode::Pop,
            Bytecode::PushUndefined,
            Bytecode::Return,
        ];
        let result = jit.compile_range(&bytecode, 0, 2, &HashMap::new(), &mut Vec::new());
        assert!(result.is_ok(), "compile failed: {:?}", result.err());

        let (mem, deopt_table, _caches) = result.unwrap();
        assert!(mem.size() > 0);
        // No deopt entries expected for PushNumber/Pop.
        assert!(deopt_table.is_empty());
    }

    #[test]
    #[ignore]

    fn compile_arithmetic() {
        let mut jit = BaselineJit::new();
        // Push 1, Push 2, Add, Return.
        let bytecode = vec![
            Bytecode::PushNumber(1.0),
            Bytecode::PushNumber(2.0),
            Bytecode::Add,
            Bytecode::Return,
        ];
        let result = jit.compile_range(&bytecode, 0, 3, &HashMap::new(), &mut Vec::new());
        assert!(result.is_ok(), "compile failed: {:?}", result.err());

        let (_mem, deopt_table, _caches) = result.unwrap();
        // Add emits 2 type guards → 2 deopt entries.
        assert_eq!(deopt_table.len(), 2);
    }

    #[test]
    #[ignore]

    fn compile_loop_pattern() {
        let mut jit = BaselineJit::new();
        // A simple loop: i=0; while(i<10) { sum+=i; i++ }
        // Bytecode:
        //   0: PushNumber(0)  ; i
        //   1: StoreLocal(0)
        //   2: PushNumber(0)  ; sum
        //   3: StoreLocal(1)
        //   4: PushNumber(0)  ; loop start -- LoadLocal(0)
        // (simplified — real bytecode would have LoadLocal, PushNumber(10), Lt, JumpIfFalse, ...)
        let bytecode = vec![
            Bytecode::LoadLocal(0),
            Bytecode::PushNumber(10.0),
            Bytecode::Lt,
            Bytecode::JumpIfFalse(8),
            Bytecode::LoadLocal(0),
            Bytecode::Inc,
            Bytecode::StoreLocal(0),
            Bytecode::Jump(0),
            Bytecode::Return,
        ];
        let result = jit.compile_range(&bytecode, 0, 7, &HashMap::new(), &mut Vec::new());
        assert!(result.is_ok(), "compile failed: {:?}", result.err());
    }

    #[test]
    fn value_layout_consistent() {
        super::super::deopt::init_value_layout();
        let n = super::super::deopt::number_discriminant();
        // Verify by checking a real Value::Number — the discriminant stored
        // in the static should match what's at offset 0 of a Number value.
        let v = Value::Number(42.0);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            )
        };
        let actual_discrim = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        assert_eq!(actual_discrim, n, "discriminant mismatch");
    }
}
