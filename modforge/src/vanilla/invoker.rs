//! Invoker controller: looks up a target by name in a
//! `sleuth::Resolver`, dispatches the call, wraps in SEH by
//! default.

use crate::patterns::sleuth::{Resolver, TargetKind};
use crate::vanilla::dispatch::{decode_return, dispatch_raw};
use crate::vanilla::sig::{ArgValue, RetValue, Signature};
use anyhow::{Result, anyhow};

/// A bound `(name, addr, sig)` triple ready to invoke. Returned
/// by `Invoker::bind`.
#[derive(Debug, Clone)]
pub struct VanillaFn {
    pub name: &'static str,
    pub addr: u64,
    pub sig: &'static Signature,
}

/// Vanilla-invoke error categories. The Display string round-trips
/// to JSON cleanly for the HTTP cmdlet.
#[derive(Debug)]
pub enum VanillaError {
    NotRegistered(String),
    NoSignature(String),
    Unresolved(String),
    Dispatch(String),
    Faulted(String),
}

impl std::fmt::Display for VanillaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRegistered(n) => write!(f, "target '{n}' not registered"),
            Self::NoSignature(n) => write!(
                f,
                "target '{n}' has no signature (registered for hook/detour, not invocation)"
            ),
            Self::Unresolved(n) => write!(f, "target '{n}' did not resolve to an address"),
            Self::Dispatch(s) => write!(f, "dispatch failed: {s}"),
            Self::Faulted(s) => write!(f, "vanilla call faulted: {s}"),
        }
    }
}

impl std::error::Error for VanillaError {}

pub struct Invoker<'r> {
    resolver: &'r Resolver,
}

impl<'r> Invoker<'r> {
    pub fn new(resolver: &'r Resolver) -> Self {
        Self { resolver }
    }

    /// Look up `name` in the registry. Returns a `VanillaFn` if
    /// the target is registered, has a signature, and resolved
    /// to a non-null address.
    pub fn bind(&self, name: &str) -> Result<VanillaFn, VanillaError> {
        let def = self
            .resolver
            .registry()
            .def(name)
            .ok_or_else(|| VanillaError::NotRegistered(name.into()))?;
        let sig = match def.kind {
            TargetKind::FunctionEntry { signature: Some(s) } => s,
            TargetKind::FunctionEntry { signature: None } => {
                return Err(VanillaError::NoSignature(name.into()));
            }
            _ => return Err(VanillaError::NoSignature(name.into())),
        };
        let addr = self
            .resolver
            .resolve(name)
            .ok_or_else(|| VanillaError::Unresolved(name.into()))?;
        Ok(VanillaFn {
            name: def.name,
            addr,
            sig,
        })
    }

    /// One-shot bind + call. Default behavior: SEH-wrapped so a
    /// fault in vanilla becomes `Err(Faulted)` instead of a
    /// process crash. Use `call_unsafe` for hot-path call sites
    /// that have proven safe.
    pub fn call(&self, name: &str, args: &[ArgValue]) -> Result<RetValue, VanillaError> {
        let bound = self.bind(name)?;
        let sig = bound.sig;
        let addr = bound.addr;
        // Pre-validate by attempting a dispatch outside the SEH
        // guard first would let arg-mismatch errors propagate as
        // Dispatch instead of swallowing as Faulted. Trade-off:
        // we accept that arg mismatches surface as Dispatch errors
        // by validating up front (no double dispatch).
        if let Err(e) = pre_validate(sig, args) {
            return Err(VanillaError::Dispatch(format!("{e}")));
        }

        // SAFETY: the dispatcher's contract is that the target
        // address actually implements the declared signature.
        // We validated arg count + kinds above; the SEH guard
        // catches any residual fault from a wrong signature or
        // a stale address.
        let result = crate::seh::guard(|| {
            // SAFETY: caller's promise per the bound TargetDef.
            unsafe { dispatch_raw(addr, sig, args) }
        });

        match result {
            Ok(Ok((rax, xmm0))) => Ok(decode_return(sig.ret, rax, xmm0)),
            Ok(Err(e)) => Err(VanillaError::Dispatch(format!("{e}"))),
            Err(seh_err) => Err(VanillaError::Faulted(format!("{seh_err:?}"))),
        }
    }

    /// SEH-less invoke. Faster (~few microseconds saved on the
    /// frame setup) but faults take the process down. Use only
    /// from call sites that have proven correctness through other
    /// means (e.g. inside a hot render trampoline).
    ///
    /// SAFETY: target's signature must actually match the
    /// function at the resolved address.
    pub unsafe fn call_unsafe(
        &self,
        name: &str,
        args: &[ArgValue],
    ) -> Result<RetValue, VanillaError> {
        let bound = self.bind(name)?;
        // SAFETY: caller's promise.
        let (rax, xmm0) = unsafe { dispatch_raw(bound.addr, bound.sig, args) }
            .map_err(|e| VanillaError::Dispatch(format!("{e}")))?;
        Ok(decode_return(bound.sig.ret, rax, xmm0))
    }
}

/// Same arity / kind checks as `dispatch_raw` performs internally,
/// surfaced as a cheap pre-flight so a mismatch returns
/// `VanillaError::Dispatch` instead of an SEH `Faulted`.
fn pre_validate(sig: &Signature, args: &[ArgValue]) -> Result<()> {
    if args.len() != sig.args.len() {
        return Err(anyhow!(
            "arg count {} mismatches signature ({})",
            args.len(),
            sig.args.len()
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
    Ok(())
}
