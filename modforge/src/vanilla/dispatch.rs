//! Win64 ABI dispatcher: marshal a `&[ArgValue]` into the right
//! registers + stack and call the target. Captures the return per
//! the Signature's `ret` kind.
//!
//! Implementation: a `global_asm!` external function with a stable
//! Win64 signature `void(Frame*)`. Inputs and outputs flow through
//! the `Frame` struct so the asm code has full control over its
//! own register allocation without conflicting with rustc.
//!
//! Win64 ABI summary the asm implements:
//!
//! - First 4 args go in registers by POSITION (not type-grouped):
//!     pos 0 -> RCX or XMM0
//!     pos 1 -> RDX or XMM1
//!     pos 2 -> R8 or XMM2
//!     pos 3 -> R9 or XMM3
//!   For non-variadic functions, only one of the two slots per
//!   position is used (per the parameter type). The dispatcher
//!   loads BOTH unconditionally; this is always safe because the
//!   callee reads whichever its signature requires.
//!
//! - Args 5+ pushed at `[rsp + 32]`, `[rsp + 40]`, etc.
//!
//! - Caller reserves 32 bytes of shadow space below the args.
//!
//! - Stack must be 16-byte aligned at the CALL instruction.
//!
//! - Integer/pointer return in RAX. Float return in XMM0.

use crate::vanilla::sig::{ArgValue, RetKind, RetValue, Signature};
use anyhow::{Result, anyhow};

/// Maximum total args supported by the dispatcher (4 register +
/// 12 stack). Bump if a real consumer needs more.
pub const MAX_ARGS: usize = 16;

/// Frame the asm dispatcher reads + writes. Layout is referenced
/// by byte offset in the asm; the order MUST match the asm code.
#[repr(C)]
struct Frame {
    fn_addr: u64,        // + 0
    n_stack: u64,        // + 8
    stack_args_ptr: u64, // +16
    regs: [u64; 4],      // +24
    rax_out: u64,        // +56
    xmm0_out: u64,       // +64
}

// SAFETY: see `dispatch_raw` doc. The asm function takes one Win64
// arg (the Frame pointer in RCX), saves callee-saved scratch
// registers, sets up the Win64 frame for `[Frame.fn_addr]`, calls,
// writes RAX + XMM0 back into the Frame, restores callee-saved
// regs, returns.
//
// Frame layout offsets (must match the #[repr(C)] above):
//   +0  fn_addr
//   +8  n_stack
//   +16 stack_args_ptr
//   +24 regs[0]
//   +32 regs[1]
//   +40 regs[2]
//   +48 regs[3]
//   +56 rax_out
//   +64 xmm0_out
core::arch::global_asm!(
    r#"
.globl modforge_vanilla_dispatch_win64
modforge_vanilla_dispatch_win64:
    push rbx
    push rsi
    push rdi
    push r12
    push r13

    // r12 = Frame ptr (callee-saved, preserved across the call).
    mov r12, rcx

    // r13 = total_sub = align_up(32 + 8 * n_stack, 16).
    mov r13, qword ptr [r12 + 8]
    shl r13, 3
    add r13, 32
    test r13, 15
    jz 1f
    add r13, 8
1:

    sub rsp, r13

    // Copy stack_args[i] -> [rsp + 32 + i*8] for i in 0..n_stack.
    mov rax, qword ptr [r12 + 16]
    mov rcx, qword ptr [r12 + 8]
    xor rdx, rdx
    test rcx, rcx
    jz 3f
2:
    mov rsi, qword ptr [rax + rdx*8]
    mov qword ptr [rsp + 32 + rdx*8], rsi
    inc rdx
    cmp rdx, rcx
    jl 2b
3:

    // Load the 4 register slots; mirror into XMM0-3.
    mov rcx, qword ptr [r12 + 24]
    mov rdx, qword ptr [r12 + 32]
    mov r8,  qword ptr [r12 + 40]
    mov r9,  qword ptr [r12 + 48]
    movq xmm0, rcx
    movq xmm1, rdx
    movq xmm2, r8
    movq xmm3, r9

    // Call.
    mov rax, qword ptr [r12 + 0]
    call rax

    // Capture results.
    mov qword ptr [r12 + 56], rax
    movq qword ptr [r12 + 64], xmm0

    // Restore stack.
    add rsp, r13

    pop r13
    pop r12
    pop rdi
    pop rsi
    pop rbx
    ret
"#
);

unsafe extern "system" {
    fn modforge_vanilla_dispatch_win64(frame: *mut Frame);
}

/// Marshall args + call. Returns the raw RAX bits and the raw
/// XMM0 bits (the caller's `Signature.ret` decides which one to
/// decode).
///
/// SAFETY: `fn_addr` must point to a function that matches `sig`.
/// Argument count and kinds must match `sig.args`. Stack args 5+
/// must fit in `MAX_ARGS - 4`. The dispatcher does NOT validate
/// the address; caller is responsible.
///
/// On Win64 the dispatcher will fault if `fn_addr` is invalid;
/// callers that want recoverable faults should wrap this in
/// `modforge::seh::guard`.
pub unsafe fn dispatch_raw(fn_addr: u64, sig: &Signature, args: &[ArgValue]) -> Result<(u64, u64)> {
    if args.len() != sig.args.len() {
        return Err(anyhow!(
            "arg count {} mismatches signature ({})",
            args.len(),
            sig.args.len()
        ));
    }
    if args.len() > MAX_ARGS {
        return Err(anyhow!(
            "arg count {} exceeds MAX_ARGS ({})",
            args.len(),
            MAX_ARGS
        ));
    }
    for (i, (kind, val)) in sig.args.iter().zip(args.iter()).enumerate() {
        if *kind != val.kind() {
            return Err(anyhow!(
                "arg[{i}] kind mismatch: sig says {:?}, got {:?}",
                kind,
                val.kind()
            ));
        }
    }

    // Materialize the four register slots as u64 bit-patterns.
    let mut regs = [0u64; 4];
    for (i, v) in args.iter().enumerate().take(4) {
        regs[i] = v.to_u64();
    }

    // Stack args (positions 4..).
    let stack_args: Vec<u64> = args.iter().skip(4).map(|v| v.to_u64()).collect();

    let mut frame = Frame {
        fn_addr,
        n_stack: stack_args.len() as u64,
        stack_args_ptr: stack_args.as_ptr() as u64,
        regs,
        rax_out: 0,
        xmm0_out: 0,
    };

    // SAFETY: caller's promise per the function safety doc.
    unsafe {
        modforge_vanilla_dispatch_win64(&mut frame as *mut Frame);
    }

    let _ = sig.ret; // ret kind is decoded by caller, not here
    Ok((frame.rax_out, frame.xmm0_out))
}

/// Decode the raw return slots into a typed RetValue per the
/// declared return kind.
pub fn decode_return(ret: RetKind, rax: u64, xmm0: u64) -> RetValue {
    match ret {
        RetKind::F32 | RetKind::F64 => RetValue::from_xmm(xmm0, ret),
        _ => RetValue::from_rax(rax, ret),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanilla::sig::{ArgKind, Signature};

    // Test targets. `extern "system"` matches the Win64 calling
    // convention the dispatcher uses.

    extern "system" fn add_i32(a: i32, b: i32) -> i32 {
        a + b
    }

    extern "system" fn sum_six(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32) -> i32 {
        a + b + c + d + e + f
    }

    extern "system" fn ptr_deref_u32(p: *const u32) -> u32 {
        unsafe { *p }
    }

    extern "system" fn add_f64(a: f64, b: f64) -> f64 {
        a + b
    }

    extern "system" fn mixed(a: i32, b: f64, c: i32) -> f64 {
        a as f64 + b + c as f64
    }

    extern "system" fn void_takes_u64(_a: u64) {}

    #[test]
    fn dispatch_add_i32_two_args_in_regs() {
        const SIG: Signature = Signature::new(&[ArgKind::I32, ArgKind::I32], RetKind::I32);
        let fn_addr = add_i32 as *const () as u64;
        let (rax, _) =
            unsafe { dispatch_raw(fn_addr, &SIG, &[ArgValue::I32(40), ArgValue::I32(2)]).unwrap() };
        assert_eq!(decode_return(RetKind::I32, rax, 0), RetValue::I32(42));
    }

    #[test]
    fn dispatch_sum_six_uses_stack_for_args_5_and_6() {
        const SIG: Signature = Signature::new(&[ArgKind::I32; 6], RetKind::I32);
        let fn_addr = sum_six as *const () as u64;
        let args = [
            ArgValue::I32(1),
            ArgValue::I32(2),
            ArgValue::I32(3),
            ArgValue::I32(4),
            ArgValue::I32(5),
            ArgValue::I32(6),
        ];
        let (rax, _) = unsafe { dispatch_raw(fn_addr, &SIG, &args).unwrap() };
        assert_eq!(decode_return(RetKind::I32, rax, 0), RetValue::I32(21));
    }

    #[test]
    fn dispatch_pointer_arg() {
        const SIG: Signature = Signature::new(&[ArgKind::Ptr], RetKind::U32);
        let v: u32 = 0xdeadbeef;
        let p = &v as *const u32 as u64;
        let fn_addr = ptr_deref_u32 as *const () as u64;
        let (rax, _) = unsafe { dispatch_raw(fn_addr, &SIG, &[ArgValue::Ptr(p)]).unwrap() };
        assert_eq!(
            decode_return(RetKind::U32, rax, 0),
            RetValue::U32(0xdeadbeef)
        );
    }

    #[test]
    fn dispatch_two_f64_args_returns_via_xmm0() {
        const SIG: Signature = Signature::new(&[ArgKind::F64, ArgKind::F64], RetKind::F64);
        let fn_addr = add_f64 as *const () as u64;
        let (_, xmm) = unsafe {
            dispatch_raw(fn_addr, &SIG, &[ArgValue::F64(1.5), ArgValue::F64(2.25)]).unwrap()
        };
        assert_eq!(decode_return(RetKind::F64, 0, xmm), RetValue::F64(3.75));
    }

    #[test]
    fn dispatch_mixed_int_float_int() {
        const SIG: Signature =
            Signature::new(&[ArgKind::I32, ArgKind::F64, ArgKind::I32], RetKind::F64);
        let fn_addr = mixed as *const () as u64;
        let args = [ArgValue::I32(10), ArgValue::F64(2.5), ArgValue::I32(7)];
        let (_, xmm) = unsafe { dispatch_raw(fn_addr, &SIG, &args).unwrap() };
        assert_eq!(decode_return(RetKind::F64, 0, xmm), RetValue::F64(19.5));
    }

    #[test]
    fn dispatch_void_return() {
        const SIG: Signature = Signature::new(&[ArgKind::U64], RetKind::Void);
        let fn_addr = void_takes_u64 as *const () as u64;
        let (rax, _) =
            unsafe { dispatch_raw(fn_addr, &SIG, &[ArgValue::U64(0xcafebabe)]).unwrap() };
        assert_eq!(decode_return(RetKind::Void, rax, 0), RetValue::Void);
    }

    #[test]
    fn dispatch_rejects_arg_count_mismatch() {
        const SIG: Signature = Signature::new(&[ArgKind::I32, ArgKind::I32], RetKind::I32);
        let r = unsafe { dispatch_raw(add_i32 as *const () as u64, &SIG, &[ArgValue::I32(1)]) };
        assert!(r.is_err());
    }

    #[test]
    fn dispatch_rejects_arg_kind_mismatch() {
        const SIG: Signature = Signature::new(&[ArgKind::I32, ArgKind::I32], RetKind::I32);
        let r = unsafe {
            dispatch_raw(
                add_i32 as *const () as u64,
                &SIG,
                &[ArgValue::I32(1), ArgValue::F32(1.0)],
            )
        };
        assert!(r.is_err());
    }
}
