//! Runtime support for C-ABI foreign function calls (`extern` blocks).
//!
//! The compiler lowers a call to an `extern`-declared function into an
//! `Instruction::ForeignCall`, which the VM executes through [`call`]. A
//! foreign symbol is resolved from a dynamically loaded shared library with
//! `libloading` and invoked by transmuting its address to a concrete
//! `extern "C" fn` shape derived from the declared signature.
//!
//! ## Safety model
//!
//! NXL cannot verify the real C signature at runtime — the declared
//! `extern` signature is trusted, exactly like linking a native binary. A
//! mismatch between the declaration and the real symbol is undefined
//! behaviour, the same hazard as a wrong prototype in C. Strings cross the
//! boundary as temporary NUL-terminated copies that live only for the
//! duration of the call; returned strings are copied immediately and are
//! never freed by NXL. Libraries are loaded once per process and never
//! unloaded.
//!
//! ## Supported surface
//!
//! Parameters and returns are the scalar types `Int` (→ `int64_t`), `Float`
//! (→ `double`), `Str` (→ `const char *`), and `Bool` (→ `int`-class 0/1),
//! with `void` returns. Arity is capped at four; anything larger, or any
//! unsupported type, is rejected at compile time by the lowering pass and
//! guarded here as a defence in depth.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{LazyLock, Mutex};

use crate::nir::FfiType;
use crate::vm::Value;

/// A catchable FFI failure: a stable error code plus a human message.
#[derive(Clone, Debug, PartialEq)]
pub struct FfiError {
    pub code: &'static str,
    pub message: String,
}

impl FfiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The shared library was found but could not be loaded.
const LOAD_FAILED: &str = "NXL11013";
/// The named symbol was not found in the library.
const SYMBOL_MISSING: &str = "NXL11014";
/// An argument could not be converted to its declared C type.
const ARG_MARSHAL: &str = "NXL11015";
/// A returned string was NULL or not valid UTF-8-safe data.
const RETURN_MARSHAL: &str = "NXL11016";
/// The signature exceeds the supported arity/type surface (defence in depth).
const UNSUPPORTED: &str = "NXL11017";

/// The maximum number of parameters a C-ABI extern declaration may declare.
/// Matches the arity handled by [`dispatch`]; the lowering pass rejects larger
/// arities at compile time.
pub const MAX_FFI_ARITY: usize = 4;

/// Loaded libraries, keyed by path, kept alive for the life of the process.
static LIBRARIES: LazyLock<Mutex<HashMap<String, libloading::Library>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolve `symbol` in `library` to a raw address, loading the library on
/// first use. The address stays valid because the library is never unloaded.
fn symbol_address(library: &str, symbol: &str) -> Result<*mut std::ffi::c_void, FfiError> {
    let mut cache = LIBRARIES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cache.contains_key(library) {
        // SAFETY: loading a library by path is safe; failures are reported.
        let loaded = unsafe { libloading::Library::new(library) }.map_err(|error| {
            FfiError::new(
                LOAD_FAILED,
                format!("cannot load external library '{library}': {error}"),
            )
        })?;
        cache.insert(library.to_owned(), loaded);
    }
    let loaded = cache.get(library).expect("library cached above");
    // SAFETY: the caller guarantees `symbol` names a real C function with the
    // declared signature; the raw address is copied out before the lock drops.
    let address =
        unsafe { loaded.get::<*mut std::ffi::c_void>(symbol.as_bytes()) }.map_err(|_| {
            FfiError::new(
                SYMBOL_MISSING,
                format!("symbol '{symbol}' not found in library '{library}'"),
            )
        })?;
    Ok(*address)
}

/// The register class an ABI scalar occupies: integer or SSE/float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbiClass {
    Int,
    Float,
}

/// The register class of a call's return value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FfiRet {
    Int,
    Float,
    Void,
}

/// A converted argument, tagged by the class it will be passed in. `Str` and
/// `Bool` are pre-folded into the integer class (`Str` as a pointer, `Bool`
/// as 0/1), so only `Int`/`Float` remain at the ABI layer.
enum RawArg {
    Int(i64),
    Float(f64),
}

impl RawArg {
    fn as_i64(&self) -> i64 {
        match self {
            RawArg::Int(value) => *value,
            RawArg::Float(_) => 0,
        }
    }

    fn as_f64(&self) -> f64 {
        match self {
            RawArg::Float(value) => *value,
            RawArg::Int(_) => 0.0,
        }
    }
}

/// A raw value returned from the foreign call, before NXL conversion.
enum RawRet {
    Int(i64),
    Float(f64),
    Void,
}

fn abi_class(ty: FfiType) -> AbiClass {
    match ty {
        FfiType::Int | FfiType::Str | FfiType::Bool => AbiClass::Int,
        FfiType::Float => AbiClass::Float,
    }
}

fn abi_ret(ret: Option<FfiType>) -> FfiRet {
    match ret {
        None => FfiRet::Void,
        Some(FfiType::Float) => FfiRet::Float,
        Some(_) => FfiRet::Int,
    }
}

// Map one ABI class token to the concrete Rust FFI type used in the
// transmuted signature: integer scalars and pointers both pass as `i64`.
macro_rules! cabi_ty {
    (Int) => {
        i64
    };
    (Float) => {
        f64
    };
}

// Extract the converted argument at position `$idx` with the type implied by
// the class token.
macro_rules! cabi_arg {
    ($raw:expr, $idx:tt, Int) => {
        $raw[$idx].as_i64()
    };
    ($raw:expr, $idx:tt, Float) => {
        $raw[$idx].as_f64()
    };
}

// Invoke `ptr` as an `extern "C" fn` whose parameter types come from the
// class list and whose return type comes from the runtime return class.
// Each return class transmutes to its own concrete shape.
macro_rules! cabi_invoke {
    ($ptr:expr, $raw:expr, [$($idx:tt = $cls:ident),*]; $ret:expr) => {{
        match $ret {
            FfiRet::Int => {
                // SAFETY: `dispatch` only reaches a shape whose class list
                // matches the declared signature; the pointer names a real C
                // symbol. The call itself is a plain `extern "C"` invocation.
                let function: extern "C" fn($(cabi_ty!($cls)),*) -> i64 =
                    unsafe { std::mem::transmute($ptr) };
                let out = function($(cabi_arg!($raw, $idx, $cls)),*);
                RawRet::Int(out)
            }
            FfiRet::Float => {
                let function: extern "C" fn($(cabi_ty!($cls)),*) -> f64 =
                    unsafe { std::mem::transmute($ptr) };
                let out = function($(cabi_arg!($raw, $idx, $cls)),*);
                RawRet::Float(out)
            }
            FfiRet::Void => {
                let function: extern "C" fn($(cabi_ty!($cls)),*) -> () =
                    unsafe { std::mem::transmute($ptr) };
                function($(cabi_arg!($raw, $idx, $cls)),*);
                RawRet::Void
            }
        }
    }};
}

/// Perform a foreign call and convert the result to an NXL [`Value`].
///
/// `arguments` are the already-evaluated NXL argument values; `parameters`
/// and `ret` are the declared ABI signature. The signature is trusted — a
/// mismatch with the real symbol is undefined behaviour.
pub fn call(
    library: &str,
    symbol: &str,
    parameters: &[FfiType],
    ret: Option<FfiType>,
    arguments: Vec<Value>,
) -> Result<Value, FfiError> {
    if arguments.len() != parameters.len() {
        return Err(FfiError::new(
            ARG_MARSHAL,
            format!(
                "'{symbol}' expects {} argument(s) but received {}",
                parameters.len(),
                arguments.len()
            ),
        ));
    }

    // Keep the NUL-terminated copies alive across the call.
    let mut keepalive: Vec<CString> = Vec::new();
    let mut raw: Vec<RawArg> = Vec::with_capacity(arguments.len());
    for (index, (value, ty)) in arguments.iter().zip(parameters).enumerate() {
        let converted = match (value, ty) {
            (Value::Integer(number), FfiType::Int) => RawArg::Int(*number),
            (Value::Float(number), FfiType::Float) => RawArg::Float(*number),
            (Value::Boolean(flag), FfiType::Bool) => RawArg::Int(*flag as i64),
            (Value::String(text), FfiType::Str) => {
                let copy = CString::new(text.as_str()).map_err(|_| {
                    FfiError::new(
                        ARG_MARSHAL,
                        format!(
                            "'{symbol}' argument {} contains an interior NUL byte",
                            index + 1
                        ),
                    )
                })?;
                let pointer = copy.as_ptr() as i64;
                keepalive.push(copy);
                RawArg::Int(pointer)
            }
            _ => {
                return Err(FfiError::new(
                    ARG_MARSHAL,
                    format!(
                        "'{symbol}' argument {} has the wrong type for the declared C signature",
                        index + 1
                    ),
                ))
            }
        };
        raw.push(converted);
    }

    let address = symbol_address(library, symbol)?;
    let classes: Vec<AbiClass> = parameters.iter().copied().map(abi_class).collect();
    let return_class = abi_ret(ret);
    let returned = dispatch(address, &classes, return_class, &raw)?;

    // Convert the raw return to NXL using the declared return type.
    let value = match (returned, ret) {
        (RawRet::Int(number), Some(FfiType::Int)) => Value::Integer(number),
        (RawRet::Int(number), Some(FfiType::Bool)) => Value::Boolean(number != 0),
        (RawRet::Int(pointer), Some(FfiType::Str)) => {
            if pointer == 0 {
                return Err(FfiError::new(
                    RETURN_MARSHAL,
                    format!("'{symbol}' returned a NULL string pointer"),
                ));
            }
            // SAFETY: a non-NULL `const char *` from a trusted C function is
            // read as a NUL-terminated C string and copied immediately.
            let text = unsafe { CStr::from_ptr(pointer as *const c_char) };
            Value::String(text.to_string_lossy().into_owned())
        }
        (RawRet::Float(number), Some(FfiType::Float)) => Value::Float(number),
        (RawRet::Void, None) => Value::Unit,
        _ => {
            return Err(FfiError::new(
                RETURN_MARSHAL,
                format!("'{symbol}' returned a value incompatible with its declared C signature"),
            ))
        }
    };
    // `keepalive` outlives the call; drop the temporary string copies now.
    drop(keepalive);
    Ok(value)
}

/// Select the concrete `extern "C" fn` shape from the ABI class pattern and
/// invoke the symbol. The class slice is matched exhaustively for arity 0-4;
/// larger arities (rejected at compile time) fall through to an error.
fn dispatch(
    address: *mut std::ffi::c_void,
    classes: &[AbiClass],
    ret: FfiRet,
    raw: &[RawArg],
) -> Result<RawRet, FfiError> {
    use AbiClass::{Float, Int};
    let result = match classes {
        [] => cabi_invoke!(address, raw, []; ret),
        [Int] => cabi_invoke!(address, raw, [0 = Int]; ret),
        [Float] => cabi_invoke!(address, raw, [0 = Float]; ret),
        [Int, Int] => cabi_invoke!(address, raw, [0 = Int, 1 = Int]; ret),
        [Int, Float] => cabi_invoke!(address, raw, [0 = Int, 1 = Float]; ret),
        [Float, Int] => cabi_invoke!(address, raw, [0 = Float, 1 = Int]; ret),
        [Float, Float] => cabi_invoke!(address, raw, [0 = Float, 1 = Float]; ret),
        [Int, Int, Int] => cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Int]; ret),
        [Int, Int, Float] => cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Float]; ret),
        [Int, Float, Int] => cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Int]; ret),
        [Int, Float, Float] => cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Float]; ret),
        [Float, Int, Int] => cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Int]; ret),
        [Float, Int, Float] => cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Float]; ret),
        [Float, Float, Int] => cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Int]; ret),
        [Float, Float, Float] => cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Float]; ret),
        [Int, Int, Int, Int] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Int, 3 = Int]; ret)
        }
        [Int, Int, Int, Float] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Int, 3 = Float]; ret)
        }
        [Int, Int, Float, Int] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Float, 3 = Int]; ret)
        }
        [Int, Int, Float, Float] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Int, 2 = Float, 3 = Float]; ret)
        }
        [Int, Float, Int, Int] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Int, 3 = Int]; ret)
        }
        [Int, Float, Int, Float] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Int, 3 = Float]; ret)
        }
        [Int, Float, Float, Int] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Float, 3 = Int]; ret)
        }
        [Int, Float, Float, Float] => {
            cabi_invoke!(address, raw, [0 = Int, 1 = Float, 2 = Float, 3 = Float]; ret)
        }
        [Float, Int, Int, Int] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Int, 3 = Int]; ret)
        }
        [Float, Int, Int, Float] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Int, 3 = Float]; ret)
        }
        [Float, Int, Float, Int] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Float, 3 = Int]; ret)
        }
        [Float, Int, Float, Float] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Int, 2 = Float, 3 = Float]; ret)
        }
        [Float, Float, Int, Int] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Int, 3 = Int]; ret)
        }
        [Float, Float, Int, Float] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Int, 3 = Float]; ret)
        }
        [Float, Float, Float, Int] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Float, 3 = Int]; ret)
        }
        [Float, Float, Float, Float] => {
            cabi_invoke!(address, raw, [0 = Float, 1 = Float, 2 = Float, 3 = Float]; ret)
        }
        _ => {
            return Err(FfiError::new(
                UNSUPPORTED,
                "external calls support at most four arguments",
            ))
        }
    };
    Ok(result)
}
