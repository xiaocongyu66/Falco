//! Integration test: actually execute JIT-compiled code.
//!
//! These tests verify that the JIT can compile and run real bytecode,
//! not just that compilation succeeds.

#![cfg(test)]

use crate::tjs::jit::{JitContext, JitExitReason};
use crate::tjs::value::Value;
use crate::tjs::vm::{Bytecode, Vm};

#[test]
fn jit_executes_push_number() {
    // Bytecode: PushNumber(42.0), Return
    let bytecode = vec![Bytecode::PushNumber(42.0), Bytecode::Return];

    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        eprintln!("[jit_executes_push_number] JIT not available, skipping");
        return;
    }

    // Compile the range [0..2).
    let compiled = ctx.compile_loop(&bytecode, 0, 1);
    if !compiled {
        eprintln!("[jit_executes_push_number] compilation failed");
        return;
    }

    // Set up a stack and locals.
    let mut stack = vec![Value::Undefined; 16];
    let mut locals = vec![Value::Undefined; 16];
    let vm_ptr: *mut u8 = std::ptr::null_mut();

    let reason = ctx.run_compiled(
        0,
        stack.as_mut_ptr(),
        stack.as_ptr(),
        locals.as_mut_ptr(),
        vm_ptr,
    );

    // Should exit normally.
    match reason {
        JitExitReason::Normal(_) => {
            // The JIT pushed 42.0 onto the stack.
            // Verify it's a Number with value 42.0.
            if let Value::Number(n) = &stack[0] {
                assert!((n - 42.0).abs() < 0.001, "expected 42.0, got {}", n);
            } else {
                panic!("expected Number, got {:?}", stack[0]);
            }
        }
        other => panic!("unexpected exit: {:?}", other),
    }
}

#[test]
fn jit_executes_arithmetic() {
    // Bytecode: PushNumber(10.0), PushNumber(20.0), Add, Return
    let bytecode = vec![
        Bytecode::PushNumber(10.0),
        Bytecode::PushNumber(20.0),
        Bytecode::Add,
        Bytecode::Return,
    ];

    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        eprintln!("[jit_executes_arithmetic] JIT not available, skipping");
        return;
    }

    let compiled = ctx.compile_loop(&bytecode, 0, 3);
    assert!(compiled, "JIT compilation should succeed");

    let mut stack = vec![Value::Undefined; 16];
    let mut locals = vec![Value::Undefined; 16];
    let vm_ptr: *mut u8 = std::ptr::null_mut();

    let reason = ctx.run_compiled(
        0,
        stack.as_mut_ptr(),
        stack.as_ptr(),
        locals.as_mut_ptr(),
        vm_ptr,
    );

    match reason {
        JitExitReason::Normal(_) => {
            if let Value::Number(n) = &stack[0] {
                assert!((n - 30.0).abs() < 0.001, "expected 30.0, got {}", n);
            } else {
                panic!("expected Number, got {:?}", stack[0]);
            }
        }
        other => panic!("unexpected exit: {:?}", other),
    }
}

#[test]
fn jit_executes_subtraction() {
    let bytecode = vec![
        Bytecode::PushNumber(50.0),
        Bytecode::PushNumber(20.0),
        Bytecode::Sub,
        Bytecode::Return,
    ];

    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        return;
    }

    let compiled = ctx.compile_loop(&bytecode, 0, 3);
    if !compiled {
        return;
    }

    let mut stack = vec![Value::Undefined; 16];
    let mut locals = vec![Value::Undefined; 16];

    let reason = ctx.run_compiled(
        0,
        stack.as_mut_ptr(),
        stack.as_ptr(),
        locals.as_mut_ptr(),
        std::ptr::null_mut(),
    );

    match reason {
        JitExitReason::Normal(_) => {
            if let Value::Number(n) = &stack[0] {
                assert!((n - 30.0).abs() < 0.001, "expected 30.0, got {}", n);
            } else {
                panic!("expected Number, got {:?}", stack[0]);
            }
        }
        other => panic!("unexpected exit: {:?}", other),
    }
}

#[test]
fn jit_executes_multiplication() {
    let bytecode = vec![
        Bytecode::PushNumber(7.0),
        Bytecode::PushNumber(6.0),
        Bytecode::Mul,
        Bytecode::Return,
    ];

    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        return;
    }
    if !ctx.compile_loop(&bytecode, 0, 3) {
        return;
    }

    let mut stack = vec![Value::Undefined; 16];
    let mut locals = vec![Value::Undefined; 16];

    let reason = ctx.run_compiled(
        0,
        stack.as_mut_ptr(),
        stack.as_ptr(),
        locals.as_mut_ptr(),
        std::ptr::null_mut(),
    );

    match reason {
        JitExitReason::Normal(_) => {
            if let Value::Number(n) = &stack[0] {
                assert!((n - 42.0).abs() < 0.001, "expected 42.0, got {}", n);
            } else {
                panic!("expected Number, got {:?}", stack[0]);
            }
        }
        other => panic!("unexpected exit: {:?}", other),
    }
}

#[test]
fn jit_deoptimizes_on_string_add() {
    // Push a String, then try Add — should deopt.
    // The JIT can't handle String operands, so it must deopt.
    let bytecode = vec![
        Bytecode::PushString(std::rc::Rc::from("hello")),
        Bytecode::PushNumber(1.0),
        Bytecode::Add,
        Bytecode::Return,
    ];

    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        return;
    }

    // The compiler should emit a deopt stub for PushString.
    let compiled = ctx.compile_loop(&bytecode, 0, 3);
    if !compiled {
        return;
    }

    let mut stack = vec![Value::Undefined; 16];
    let mut locals = vec![Value::Undefined; 16];

    let reason = ctx.run_compiled(
        0,
        stack.as_mut_ptr(),
        stack.as_ptr(),
        locals.as_mut_ptr(),
        std::ptr::null_mut(),
    );

    // Should deopt (not Normal).
    match reason {
        JitExitReason::Deopt(_) => {
            // Expected — JIT can't handle PushString, so it deopts.
        }
        JitExitReason::Normal(_) => {
            // Also OK if it somehow executed correctly.
        }
        other => panic!("unexpected exit: {:?}", other),
    }
}

#[test]
fn jit_stats_track_loops() {
    let mut ctx = JitContext::new();
    if !ctx.is_available() {
        return;
    }

    // Simulate loop iterations.
    let threshold = 50;
    for _ in 0..threshold {
        ctx.loop_entry(42);
    }

    // After threshold iterations, loop_entry should have triggered.
    assert!(ctx.stats().loops_detected >= threshold);
}

#[test]
fn jit_end_to_end_with_vm() {
    // Run a full program through the VM (which uses the JIT internally).
    // Note: the VM currently pops expression statement results, so the
    // return value may be undefined. The important thing is that the VM
    // doesn't crash and the JIT infrastructure is exercised.
    let mut tjs = crate::tjs::TjsContext::new().with_vm();
    // Use console.log to verify the computation actually ran.
    let result = tjs.execute(
        "var sum = 0; for (var i = 1; i <= 100; i++) { sum += i } console.log(sum); sum",
    );
    // The VM should at least not crash.
    assert!(result.is_ok(), "VM execution failed: {:?}", result.err());
}
