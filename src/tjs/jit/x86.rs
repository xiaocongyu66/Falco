//! x86-64 instruction encoder.
//!
//! Emits raw machine code bytes for a subset of x86-64 sufficient for
//! the baseline JIT:
//!
//! - Integer arithmetic: MOV, ADD, SUB, CMP, INC, DEC, AND, OR, XOR, SHL, SHR
//! - Memory access: MOV reg,[base+disp], MOV [base+disp],reg
//! - SSE2 floating point: MOVSD, ADDSD, SUBSD, MULSD, DIVSD, UCOMISD, CVTSI2SD
//! - Control flow: JMP, Jcc (rel32), CALL, RET
//! - Stack: PUSH, POP, LEAVE
//! - Atomic: CMPXCHG (for inline caches)
//!
//! Register allocation is manual — the JIT uses a fixed convention:
//!
//! | Register | Usage                                    |
//! |----------|------------------------------------------|
//! | rdi      | arg 1: vm pointer                        |
//! | rsi      | arg 2: stack-top Value pointer (mutable) |
//! | rdx      | arg 3: stack-base Value pointer          |
//! | rcx      | arg 4: locals pointer                    |
//! | r8-r11   | scratch / VM calls                       |
//! | r12-r15  | callee-saved (saved/restored by JIT)     |
//! | rax      | return value / IC slot pointer           |
//! | rbx      | reserved (callee-saved, avoid)           |
//! | xmm0-xmm3| unboxed f64 working registers            |

/// General-purpose 64-bit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

/// SSE2 128-bit register (used for f64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XmmReg {
    Xmm0 = 0,
    Xmm1 = 1,
    Xmm2 = 2,
    Xmm3 = 3,
    Xmm4 = 4,
    Xmm5 = 5,
    Xmm6 = 6,
    Xmm7 = 7,
    Xmm8 = 8,
    Xmm9 = 9,
    Xmm10 = 10,
    Xmm11 = 11,
    Xmm12 = 12,
    Xmm13 = 13,
    Xmm14 = 14,
    Xmm15 = 15,
}

/// Condition codes for Jcc / SETcc / CMOVcc.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Cond {
    O = 0x0,  // Overflow
    No = 0x1,
    B = 0x2,  // Below (unsigned <)
    Nb = 0x3,
    E = 0x4,  // Equal
    Ne = 0x5,
    Be = 0x6, // Below or equal (unsigned <=)
    A = 0x7,  // Above (unsigned >)
    S = 0x8,
    Ns = 0x9,
    P = 0xA,
    Np = 0xB,
    L = 0xC,  // Less (signed <)
    Nl = 0xD,
    Le = 0xE, // Less or equal (signed <=)
    G = 0xF,  // Greater (signed >)
}

/// A machine code buffer — emits raw x86-64 bytes.
pub struct CodeEmitter {
    pub(crate) code: Vec<u8>,
}

impl Default for CodeEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeEmitter {
    pub fn new() -> Self {
        Self {
            code: Vec::with_capacity(8192),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.code
    }

    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    pub fn clear(&mut self) {
        self.code.clear();
    }

    // ── Low-level byte emission ────────────────────────────────────────

    #[inline(always)]
    pub fn emit_byte(&mut self, b: u8) {
        self.code.push(b);
    }

    #[inline(always)]
    pub fn emit_word(&mut self, w: u16) {
        self.code.extend_from_slice(&w.to_le_bytes());
    }

    #[inline(always)]
    pub fn emit_dword(&mut self, d: u32) {
        self.code.extend_from_slice(&d.to_le_bytes());
    }

    #[inline(always)]
    pub fn emit_qword(&mut self, q: u64) {
        self.code.extend_from_slice(&q.to_le_bytes());
    }

    /// Emit an i32 as 4 LE bytes (signed dword).
    #[inline(always)]
    pub fn emit_i32(&mut self, v: i32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    // ── REX prefix ────────────────────────────────────────────────────

    /// Emit a REX prefix.
    ///
    /// `w` = 64-bit operand size, `r` = reg field extension (bit 3),
    /// `x` = SIB index extension, `b` = r/m or opcode reg extension.
    #[inline(always)]
    pub fn rex(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let mut byte = 0x40u8;
        if w {
            byte |= 0x08;
        }
        if r >= 8 {
            byte |= 0x04;
        }
        if x >= 8 {
            byte |= 0x02;
        }
        if b >= 8 {
            byte |= 0x01;
        }
        if byte != 0x40 {
            self.emit_byte(byte);
        }
    }

    /// Always emit a REX.W prefix (force 64-bit operand).
    #[inline(always)]
    pub fn rex_w(&mut self) {
        self.emit_byte(0x48);
    }

    // ── ModR/M and SIB byte helpers ───────────────────────────────────

    /// Emit a ModR/M byte: mod(2) | reg(3) | rm(3).
    #[inline(always)]
    pub fn modrm(&mut self, mod_bits: u8, reg: u8, rm: u8) {
        let byte = ((mod_bits & 0x3) << 6) | ((reg & 0x7) << 3) | (rm & 0x7);
        self.emit_byte(byte);
    }

    /// Emit a SIB byte: scale(2) | index(3) | base(3).
    #[inline(always)]
    pub fn sib(&mut self, scale: u8, index: u8, base: u8) {
        let byte = ((scale & 0x3) << 6) | ((index & 0x7) << 3) | (base & 0x7);
        self.emit_byte(byte);
    }

    /// Emit a displacement for a ModR/M with [base+disp] addressing.
    ///
    /// - disp == 0 and base != rbp/r13 → no disp
    /// - -128 <= disp <= 127 → 1-byte disp
    /// - else → 4-byte disp
    fn emit_disp(&mut self, base: Reg, disp: i32) {
        // rbp/r13 require explicit disp8=0 when disp==0 (mod=00 is interpreted
        // as [disp32] for rbp/r13, so we must use mod=01 with disp8=0).
        let needs_disp = disp != 0 || matches!(base, Reg::Rbp | Reg::R13);
        if !needs_disp {
            return;
        }
        if (-128..=127).contains(&disp) {
            self.emit_byte(disp as u8);
        } else {
            self.emit_i32(disp);
        }
    }

    /// Emit ModR/M for [base+disp] addressing with a register destination.
    fn modrm_mem(&mut self, reg: u8, base: Reg, disp: i32) {
        let b = base as u8;
        let needs_disp = disp != 0 || matches!(base, Reg::Rbp | Reg::R13);
        let (mod_bits, _) = if needs_disp {
            if (-128..=127).contains(&disp) {
                (0x01, 1) // disp8
            } else {
                (0x02, 4) // disp32
            }
        } else {
            (0x00, 0)
        };
        self.modrm(mod_bits, reg & 0x7, b & 0x7);
        self.emit_disp(base, disp);
    }

    // ── Integer MOV ───────────────────────────────────────────────────

    /// MOV r64, imm64
    pub fn mov_imm64(&mut self, dst: Reg, imm: u64) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0xB8 + (d & 7));
        self.emit_qword(imm);
    }

    /// MOV r64, imm32 (zero-extended)
    pub fn mov_imm32(&mut self, dst: Reg, imm: u32) {
        let d = dst as u8;
        self.rex(false, 0, 0, d); // 32-bit operand, but reg can be extended
        self.emit_byte(0xB8 + (d & 7));
        self.emit_dword(imm);
    }

    /// MOV r64, r64
    pub fn mov_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x89);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// MOV r64, [base+disp]
    pub fn load_mem(&mut self, dst: Reg, base: Reg, disp: i32) {
        let d = dst as u8;
        let b = base as u8;
        self.rex(true, d, 0, b);
        self.emit_byte(0x8B);
        self.modrm_mem(d, base, disp);
    }

    /// MOV [base+disp], r64
    pub fn store_mem(&mut self, base: Reg, disp: i32, src: Reg) {
        let s = src as u8;
        let b = base as u8;
        self.rex(true, s, 0, b);
        self.emit_byte(0x89);
        self.modrm_mem(s, base, disp);
    }

    /// MOV r64, [reg + reg*scale + disp]  (SIB addressing)
    ///
    /// Useful for array access: `arr[i]` → `arr_base + i*sizeof`.
    pub fn load_sib(
        &mut self,
        dst: Reg,
        base: Reg,
        index: Reg,
        scale: u8, // 1, 2, 4, or 8
        disp: i32,
    ) {
        let d = dst as u8;
        let b = base as u8;
        let i = index as u8;
        self.rex(true, d, i, b);
        self.emit_byte(0x8B);
        let scale_bits = match scale {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => panic!("invalid scale {}", scale),
        };
        let mod_bits = if disp == 0 && !matches!(base, Reg::Rbp | Reg::R13) {
            0x00
        } else if (-128..=127).contains(&disp) {
            0x01
        } else {
            0x02
        };
        self.modrm(mod_bits, d & 0x7, 0x4); // rm=0x4 → SIB follows
        self.sib(scale_bits, i & 0x7, b & 0x7);
        if mod_bits == 0x01 {
            self.emit_byte(disp as u8);
        } else if mod_bits == 0x02 {
            self.emit_i32(disp);
        }
    }

    /// MOV [reg + reg*scale + disp], r64  (SIB store)
    pub fn store_sib(
        &mut self,
        base: Reg,
        index: Reg,
        scale: u8,
        disp: i32,
        src: Reg,
    ) {
        let s = src as u8;
        let b = base as u8;
        let i = index as u8;
        self.rex(true, s, i, b);
        self.emit_byte(0x89);
        let scale_bits = match scale {
            1 => 0,
            2 => 1,
            4 => 2,
            8 => 3,
            _ => panic!("invalid scale {}", scale),
        };
        let mod_bits = if disp == 0 && !matches!(base, Reg::Rbp | Reg::R13) {
            0x00
        } else if (-128..=127).contains(&disp) {
            0x01
        } else {
            0x02
        };
        self.modrm(mod_bits, s & 0x7, 0x4);
        self.sib(scale_bits, i & 0x7, b & 0x7);
        if mod_bits == 0x01 {
            self.emit_byte(disp as u8);
        } else if mod_bits == 0x02 {
            self.emit_i32(disp);
        }
    }

    // ── Integer arithmetic ────────────────────────────────────────────

    /// ADD r64, r64
    pub fn add_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x01);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// ADD r64, imm32
    pub fn add_imm32(&mut self, dst: Reg, imm: i32) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0x81);
        self.modrm(0x3, 0, d & 0x7);
        self.emit_i32(imm);
    }

    /// SUB r64, r64
    pub fn sub_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x29);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// SUB r64, imm32
    pub fn sub_imm32(&mut self, dst: Reg, imm: i32) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0x81);
        self.modrm(0x3, 5, d & 0x7);
        self.emit_i32(imm);
    }

    /// CMP r64, r64
    pub fn cmp_reg(&mut self, a: Reg, b: Reg) {
        let aa = a as u8;
        let bb = b as u8;
        self.rex(true, bb, 0, aa);
        self.emit_byte(0x39);
        self.modrm(0x3, bb & 0x7, aa & 0x7);
    }

    /// CMP r64, imm32
    pub fn cmp_imm32(&mut self, reg: Reg, imm: i32) {
        let r = reg as u8;
        self.rex(true, 0, 0, r);
        self.emit_byte(0x81);
        self.modrm(0x3, 7, r & 0x7);
        self.emit_i32(imm);
    }

    /// INC r64
    pub fn inc_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        self.rex(true, 0, 0, r);
        self.emit_byte(0xFF);
        self.modrm(0x3, 0, r & 0x7);
    }

    /// DEC r64
    pub fn dec_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        self.rex(true, 0, 0, r);
        self.emit_byte(0xFF);
        self.modrm(0x3, 1, r & 0x7);
    }

    /// AND r64, r64
    pub fn and_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x21);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// AND r64, imm32
    pub fn and_imm32(&mut self, dst: Reg, imm: i32) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0x81);
        self.modrm(0x3, 4, d & 0x7);
        self.emit_i32(imm);
    }

    /// OR r64, r64
    pub fn or_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x09);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// XOR r64, r64 (zero a register when dst==src)
    pub fn xor_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, s, 0, d);
        self.emit_byte(0x31);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// SHL r64, imm8
    pub fn shl_imm8(&mut self, dst: Reg, count: u8) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0xC1);
        self.modrm(0x3, 4, d & 0x7);
        self.emit_byte(count);
    }

    /// SHR r64, imm8 (logical shift right)
    pub fn shr_imm8(&mut self, dst: Reg, count: u8) {
        let d = dst as u8;
        self.rex(true, 0, 0, d);
        self.emit_byte(0xC1);
        self.modrm(0x3, 5, d & 0x7);
        self.emit_byte(count);
    }

    /// MUL r64 (unsigned: rdx:rax = rax * rm64)
    pub fn mul_reg(&mut self, src: Reg) {
        let s = src as u8;
        self.rex(true, 0, 0, s);
        self.emit_byte(0xF7);
        self.modrm(0x3, 4, s & 0x7);
    }

    /// IMUL r64, r64 (signed: dst = dst * src, no overflow detection)
    pub fn imul_reg(&mut self, dst: Reg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.rex(true, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0xAF);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    // ── Stack ops ──────────────────────────────────────────────────────

    /// PUSH r64
    pub fn push_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        self.rex(false, 0, 0, r);
        self.emit_byte(0x50 + (r & 7));
    }

    /// POP r64
    pub fn pop_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        self.rex(false, 0, 0, r);
        self.emit_byte(0x58 + (r & 7));
    }

    /// LEAVE (mov rsp, rbp; pop rbp) — for cleaning up frames.
    pub fn leave(&mut self) {
        self.emit_byte(0xC9);
    }

    // ── Control flow ──────────────────────────────────────────────────

    /// RET (near return)
    pub fn ret(&mut self) {
        self.emit_byte(0xC3);
    }

    /// JMP rel32
    pub fn jmp_rel32(&mut self, offset: i32) {
        self.emit_byte(0xE9);
        self.emit_i32(offset);
    }

    /// Emit a rel32 jump with a placeholder offset; returns the byte offset
    /// of the placeholder (for later patching via `patch_rel32`).
    pub fn jmp_placeholder(&mut self) -> usize {
        self.emit_byte(0xE9);
        let pos = self.code.len();
        self.emit_i32(0);
        pos
    }

    /// Jcc rel32 (conditional jump)
    pub fn jcc_rel32(&mut self, cond: Cond, offset: i32) {
        self.emit_byte(0x0F);
        self.emit_byte(0x80 | (cond as u8));
        self.emit_i32(offset);
    }

    /// Emit Jcc with a placeholder offset; returns the byte offset of the
    /// placeholder for later patching.
    pub fn jcc_placeholder(&mut self, cond: Cond) -> usize {
        self.emit_byte(0x0F);
        self.emit_byte(0x80 | (cond as u8));
        let pos = self.code.len();
        self.emit_i32(0);
        pos
    }

    /// Patch a rel32 placeholder at `patch_pos` to jump to `target_pos`.
    ///
    /// The rel32 is interpreted as: target = next_instruction_addr + rel32
    ///                              = (patch_pos + 4) + rel32
    /// So rel32 = target_pos - (patch_pos + 4).
    pub fn patch_rel32(&mut self, patch_pos: usize, target_pos: usize) {
        let rel = target_pos as i64 - patch_pos as i64 - 4;
        let bytes = self.code.as_mut_slice();
        bytes[patch_pos..patch_pos + 4].copy_from_slice(&(rel as i32).to_le_bytes());
    }

    /// CALL r64 (indirect call through register)
    pub fn call_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        self.rex(false, 0, 0, r);
        self.emit_byte(0xFF);
        self.modrm(0x3, 2, r & 0x7);
    }

    /// CALL [base+disp] (indirect call through memory)
    pub fn call_mem(&mut self, base: Reg, disp: i32) {
        let b = base as u8;
        self.rex(false, 0, 0, b);
        self.emit_byte(0xFF);
        self.modrm_mem(2, base, disp);
    }

    /// CALL rel32 (relative call)
    pub fn call_rel32(&mut self, offset: i32) {
        self.emit_byte(0xE8);
        self.emit_i32(offset);
    }

    /// NOP
    pub fn nop(&mut self) {
        self.emit_byte(0x90);
    }

    /// INT3 (debugger breakpoint)
    pub fn int3(&mut self) {
        self.emit_byte(0xCC);
    }

    // ── SSE2 floating point ───────────────────────────────────────────

    /// MOVQ xmm, r64 (move 64 bits from GP register to XMM register)
    pub fn movq_reg_to_xmm(&mut self, dst: XmmReg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0x66);
        self.rex(true, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x6E);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// MOVQ r64, xmm (move 64 bits from XMM to GP register)
    pub fn movq_xmm_to_reg(&mut self, dst: Reg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0x66);
        self.rex(true, s, 0, d);
        self.emit_byte(0x0F);
        self.emit_byte(0x7E);
        self.modrm(0x3, s & 0x7, d & 0x7);
    }

    /// MOVSD xmm, [base+disp] (load double from memory)
    pub fn movsd_load(&mut self, dst: XmmReg, base: Reg, disp: i32) {
        let d = dst as u8;
        let b = base as u8;
        self.emit_byte(0xF2);
        self.rex(false, d, 0, b);
        self.emit_byte(0x0F);
        self.emit_byte(0x10);
        self.modrm_mem(d, base, disp);
    }

    /// MOVSD [base+disp], xmm (store double to memory)
    pub fn movsd_store(&mut self, base: Reg, disp: i32, src: XmmReg) {
        let s = src as u8;
        let b = base as u8;
        self.emit_byte(0xF2);
        self.rex(false, s, 0, b);
        self.emit_byte(0x0F);
        self.emit_byte(0x11);
        self.modrm_mem(s, base, disp);
    }

    /// ADDSD xmm_dst, xmm_src  →  xmm_dst = xmm_dst + xmm_src
    ///
    /// Intel encoding: reg field = destination, rm field = source.
    pub fn addsd(&mut self, dst: XmmReg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(false, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x58);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// SUBSD xmm_dst, xmm_src  →  xmm_dst = xmm_dst - xmm_src
    pub fn subsd(&mut self, dst: XmmReg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(false, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x5C);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// MULSD xmm_dst, xmm_src  →  xmm_dst = xmm_dst * xmm_src
    pub fn mulsd(&mut self, dst: XmmReg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(false, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x59);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// DIVSD xmm_dst, xmm_src  →  xmm_dst = xmm_dst / xmm_src
    pub fn divsd(&mut self, dst: XmmReg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(false, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x5E);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// XORPS xmm_dst, xmm_src (zero an XMM register when dst==src)
    pub fn xorps(&mut self, dst: XmmReg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0x66);
        self.rex(false, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x57);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// UCOMISD xmm_a, xmm_b  →  sets flags based on (xmm_a - xmm_b)
    pub fn ucomisd(&mut self, a: XmmReg, b: XmmReg) {
        let aa = a as u8;
        let bb = b as u8;
        self.emit_byte(0x66);
        self.rex(false, aa, 0, bb);
        self.emit_byte(0x0F);
        self.emit_byte(0x2E);
        self.modrm(0x3, aa & 0x7, bb & 0x7);
    }

    /// CVTSI2SD xmm, r64 (convert signed 64-bit int to double)
    pub fn cvtsi2sd(&mut self, dst: XmmReg, src: Reg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(true, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x2A);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    /// CVTSD2SI r64, xmm (convert double to signed 64-bit int, truncating)
    pub fn cvtsd2si(&mut self, dst: Reg, src: XmmReg) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_byte(0xF2);
        self.rex(true, d, 0, s);
        self.emit_byte(0x0F);
        self.emit_byte(0x2D);
        self.modrm(0x3, d & 0x7, s & 0x7);
    }

    // ── Function prologue / epilogue ──────────────────────────────────

    /// Standard function prologue: push callee-saved registers, set up frame.
    ///
    /// Saves: rbx, r12, r13, r14, r15 (callee-saved per System V AMD64 ABI).
    pub fn prologue(&mut self) {
        self.push_reg(Reg::Rbx);
        self.push_reg(Reg::R12);
        self.push_reg(Reg::R13);
        self.push_reg(Reg::R14);
        self.push_reg(Reg::R15);
    }

    /// Standard function epilogue: restore callee-saved registers, return.
    pub fn epilogue(&mut self) {
        self.pop_reg(Reg::R15);
        self.pop_reg(Reg::R14);
        self.pop_reg(Reg::R13);
        self.pop_reg(Reg::R12);
        self.pop_reg(Reg::Rbx);
        self.ret();
    }

    /// MOV rax, imm64 (return value setup)
    pub fn set_return_u64(&mut self, val: u64) {
        self.mov_imm64(Reg::Rax, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitter_basic() {
        let mut e = CodeEmitter::new();
        e.ret();
        assert_eq!(e.bytes(), &[0xC3]);
    }

    #[test]
    fn emitter_mov_imm64() {
        let mut e = CodeEmitter::new();
        // mov rax, 0x123456789ABCDEF0
        // 48 B8 F0 DE BC 9A 78 56 34 12
        e.mov_imm64(Reg::Rax, 0x123456789ABCDEF0);
        assert_eq!(
            e.bytes(),
            &[0x48, 0xB8, 0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12]
        );
    }

    #[test]
    fn emitter_mov_imm64_extended_reg() {
        let mut e = CodeEmitter::new();
        // mov r15, 0x100
        // 49 BF 00 01 00 00 00 00 00 00
        e.mov_imm64(Reg::R15, 0x100);
        assert_eq!(
            e.bytes(),
            &[0x49, 0xBF, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn emitter_xor_rax_rax() {
        let mut e = CodeEmitter::new();
        // xor rax, rax = 48 31 C0
        e.xor_reg(Reg::Rax, Reg::Rax);
        assert_eq!(e.bytes(), &[0x48, 0x31, 0xC0]);
    }

    #[test]
    fn emitter_ret() {
        let mut e = CodeEmitter::new();
        e.ret();
        assert_eq!(e.bytes(), &[0xC3]);
    }

    #[test]
    fn emitter_addsd() {
        let mut e = CodeEmitter::new();
        // addsd xmm0, xmm1 = F2 0F 58 C1
        e.addsd(XmmReg::Xmm0, XmmReg::Xmm1);
        assert_eq!(e.bytes(), &[0xF2, 0x0F, 0x58, 0xC1]);
    }

    #[test]
    fn emitter_load_mem_simple() {
        let mut e = CodeEmitter::new();
        // mov rax, [rsi] = 48 8B 06
        e.load_mem(Reg::Rax, Reg::Rsi, 0);
        assert_eq!(e.bytes(), &[0x48, 0x8B, 0x06]);
    }

    #[test]
    fn emitter_load_mem_disp8() {
        let mut e = CodeEmitter::new();
        // mov rax, [rsi+16] = 48 8B 46 10
        e.load_mem(Reg::Rax, Reg::Rsi, 16);
        assert_eq!(e.bytes(), &[0x48, 0x8B, 0x46, 0x10]);
    }

    #[test]
    fn emitter_load_mem_disp32() {
        let mut e = CodeEmitter::new();
        // mov rax, [rsi+0x1000] = 48 8B 86 00 10 00 00
        e.load_mem(Reg::Rax, Reg::Rsi, 0x1000);
        assert_eq!(
            e.bytes(),
            &[0x48, 0x8B, 0x86, 0x00, 0x10, 0x00, 0x00]
        );
    }

    #[test]
    fn emitter_prologue_epilogue() {
        let mut e = CodeEmitter::new();
        e.prologue();
        // push rbx = 53
        // push r12 = 41 54
        // push r13 = 41 55
        // push r14 = 41 56
        // push r15 = 41 57
        assert_eq!(e.bytes(), &[0x53, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57]);
        e.clear();
        e.epilogue();
        // pop r15 = 41 5F
        // pop r14 = 41 5E
        // pop r13 = 41 5D
        // pop r12 = 41 5C
        // pop rbx = 5B
        // ret    = C3
        assert_eq!(e.bytes(), &[0x41, 0x5F, 0x41, 0x5E, 0x41, 0x5D, 0x41, 0x5C, 0x5B, 0xC3]);
    }

    #[test]
    fn emitter_patch_rel32() {
        let mut e = CodeEmitter::new();
        // jmp placeholder at offset 0
        let p = e.jmp_placeholder();
        // some padding
        e.nop();
        e.nop();
        e.nop();
        // patch to jump here
        let target = e.len();
        e.patch_rel32(p, target);
        // Verify the patched offset is target - (p + 4) = 7 - 4 = 3
        let bytes = e.bytes();
        let off = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert_eq!(off, 3);
    }
}
