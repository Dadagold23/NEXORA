//! C-ABI `extern` calls, unsupported on Android.
//!
//! The desktop compiler resolves `extern` declarations at runtime with
//! `libloading`, which opens arbitrary shared objects. An APK has no library
//! search path the app can rely on, and letting downloaded code run inside the
//! app process is not something a mobile target should do by default. So this
//! module keeps the compiler's FFI surface compiling without enabling it.
//!
//! The public shape must match `compiler/src/cabi.rs` closely enough for
//! `nir.rs` (arity validation) and `vm.rs` (the `ForeignCall` opcode) to build.
//! A call fails as a catchable diagnostic instead of aborting the program.

use crate::nir::FfiType;
use crate::vm::Value;

/// A catchable FFI failure: the same shape the desktop implementation returns.
#[derive(Clone, Debug, PartialEq)]
pub struct FfiError {
    pub code: &'static str,
    pub message: String,
}

/// The maximum number of parameters a C-ABI extern declaration may declare.
///
/// Must stay in step with `cabi::MAX_FFI_ARITY` in the desktop compiler: the
/// lowering pass uses it to reject declarations, whether or not this target can
/// execute them.
pub const MAX_FFI_ARITY: usize = 4;

/// Refuse a foreign call, catchably.
///
/// The signature mirrors the desktop `cabi::call`, so the VM needs no
/// conditional code for this target.
pub fn call(
    _library: &str,
    symbol: &str,
    _parameters: &[FfiType],
    _ret: Option<FfiType>,
    _arguments: Vec<Value>,
) -> Result<Value, FfiError> {
    Err(FfiError {
        code: "NXL11017",
        message: format!("extern call to '{symbol}' is not supported on this platform"),
    })
}
