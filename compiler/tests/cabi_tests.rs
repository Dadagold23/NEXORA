//! Integration tests for C-ABI `extern` bindings.
//!
//! Every dynamic call targets a library the OS itself ships — `kernel32.dll`
//! on Windows, `libc.so.6` on Linux, `libSystem.B.dylib` on macOS — so the
//! suite exercises real dynamic loading without shipping native fixtures and
//! without touching the network. Everything runs on the VM, the only target
//! that can perform native calls.

use nxl_compiler::source::SourceFile;
use nxl_compiler::vm;

/// Run an NXL program on the VM and return what it printed.
fn run(name: &str, source: &str) -> Vec<String> {
    let file = SourceFile::new(name, source);
    let result = vm::run(&file).expect("program must run");
    result.output
}

/// The OS library used for platform smoke tests.
#[cfg(windows)]
const TICK_LIB: &str = "kernel32.dll";
#[cfg(target_os = "linux")]
const TICK_LIB: &str = "libc.so.6";
#[cfg(target_os = "macos")]
const TICK_LIB: &str = "libSystem.B.dylib";

#[test]
fn extern_zero_arg_int_call_on_os_library() {
    // Windows: GetTickCount64() -> u64 ms since boot.
    // Linux: getppid() -> pid_t, always non-negative.
    // macOS: getpid() -> pid_t, always positive.
    #[cfg(windows)]
    let source_text = "extern \"kernel32.dll\" {\n\
         fn GetTickCount64() -> Int;\n\
     }\n\
     print(GetTickCount64() >= 0);\n";
    #[cfg(target_os = "linux")]
    let source_text = "extern \"libc.so.6\" {\n\
         fn getppid() -> Int;\n\
     }\n\
     print(getppid() >= 0);\n";
    #[cfg(target_os = "macos")]
    let source_text = "extern \"libSystem.B.dylib\" {\n\
         fn getpid() -> Int;\n\
     }\n\
     print(getpid() > 0);\n";
    let output = run("ffi_tick.nxl", source_text);
    assert_eq!(output, vec!["true"]);
}

#[cfg(target_os = "linux")]
#[test]
fn extern_strlen_returns_int_on_linux() {
    let output = run(
        "ffi_strlen.nxl",
        "extern \"libc.so.6\" {\n\
             fn strlen(s: String) -> Int;\n\
         }\n\
         print(strlen(\"hello\"));\n\
         print(strlen(\"\"));\n",
    );
    assert_eq!(output, vec!["5", "0"]);
}

#[cfg(target_os = "macos")]
#[test]
fn extern_strlen_returns_int_on_macos() {
    let output = run(
        "ffi_strlen.nxl",
        "extern \"libSystem.B.dylib\" {\n\
             fn strlen(s: String) -> Int;\n\
         }\n\
         print(strlen(\"hello\"));\n\
         print(strlen(\"\"));\n",
    );
    assert_eq!(output, vec!["5", "0"]);
}

#[cfg(windows)]
#[test]
fn extern_str_param_and_string_return_on_windows() {
    // lstrlenA: LPCSTR -> int, the classic string-in C call.
    // GetProcAddress(GetModuleHandleA("kernel32.dll"), "GetTickCount64"):
    // two string parameters returning a pointer as Int — exercises the
    // integer register class carrying a pointer value.
    let output = run(
        "ffi_strings.nxl",
        "extern \"kernel32.dll\" {\n\
             fn lstrlenA(s: String) -> Int;\n\
             fn GetModuleHandleA(name: String) -> Int;\n\
             fn GetProcAddress(handle: Int, name: String) -> Int;\n\
         }\n\
         print(lstrlenA(\"hello\"));\n\
         print(lstrlenA(\"\"));\n\
         let kernel = GetModuleHandleA(\"kernel32.dll\");\n\
         let address = GetProcAddress(kernel, \"GetTickCount64\");\n\
         print(address != 0);\n",
    );
    assert_eq!(output, vec!["5", "0", "true"]);
}

#[cfg(windows)]
#[test]
fn extern_integer_params_promote_widely_on_windows() {
    // MulDiv(a, b, c) = a * b / c rounded to nearest — three Int parameters
    // in, Int out.
    let output = run(
        "ffi_muldiv.nxl",
        "extern \"kernel32.dll\" {\n\
             fn MulDiv(a: Int, b: Int, c: Int) -> Int;\n\
         }\n\
         print(MulDiv(3, 4, 2));\n\
         print(MulDiv(7, 1, 3));\n",
    );
    assert_eq!(output, vec!["6", "2"]);
}

#[test]
fn extern_missing_library_is_a_catchable_error() {
    let output = run(
        "ffi_missing_lib.nxl",
        "extern \"definitely-not-a-real-library-xyz.dll\" {\n\
             fn mystery() -> Int;\n\
         }\n\
         try {\n\
             print(mystery());\n\
         } catch e {\n\
             print(e);\n\
         }\n",
    );
    assert_eq!(output.len(), 1, "one caught error, got {output:?}");
    assert!(
        output[0].contains("NXL11013"),
        "error should carry the load-failure code: {}",
        output[0]
    );
    assert!(
        output[0].contains("cannot load external library"),
        "unexpected message: {}",
        output[0]
    );
}

#[test]
fn extern_missing_symbol_is_a_catchable_error() {
    let output = run(
        "ffi_missing_symbol.nxl",
        &format!(
            "extern \"{TICK_LIB}\" {{\n\
                 fn NxLNoSuchSymbolAnywhere() -> Int;\n\
             }}\n\
             try {{\n\
                 print(NxLNoSuchSymbolAnywhere());\n\
             }} catch e {{\n\
                 print(e);\n\
             }}\n"
        ),
    );
    assert_eq!(output.len(), 1, "one caught error, got {output:?}");
    assert!(
        output[0].contains("NXL11014"),
        "error should carry the missing-symbol code: {}",
        output[0]
    );
    assert!(
        output[0].contains("not found in library"),
        "unexpected message: {}",
        output[0]
    );
}

#[test]
fn extern_interior_nul_string_is_a_catchable_error() {
    let lib = if cfg!(target_os = "windows") {
        "kernel32.dll"
    } else if cfg!(target_os = "macos") {
        "libSystem.B.dylib"
    } else {
        "libc.so.6"
    };
    let output = run(
        "ffi_nul.nxl",
        &format!(
            "import std.crypto;\n\
             extern \"{lib}\" {{\n\
                 fn strlen(s: String) -> Int;\n\
             }}\n\
             let bad = std.crypto.hex_decode(\"610062\");\n\
             try {{\n\
                 print(strlen(bad));\n\
             }} catch e {{\n\
                 print(e);\n\
             }}\n"
        ),
    );
    assert_eq!(output.len(), 1, "one caught error, got {output:?}");
    assert!(
        output[0].contains("NXL11015"),
        "error should carry the argument-marshalling code: {}",
        output[0]
    );
}

#[test]
fn extern_wrong_argument_type_is_a_compile_error() {
    let file = SourceFile::new(
        "ffi_bad_arg.nxl",
        "extern \"kernel32.dll\" {\n\
             fn lstrlenA(s: String) -> Int;\n\
         }\n\
         print(lstrlenA(42));\n",
    );
    let error = nxl_compiler::compile_to_nir(&file).expect_err("Int cannot pass as a C string");
    assert_eq!(error.code, "NXL2001");
    assert!(
        error.message.contains("expected String"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_wrong_arity_is_a_compile_error() {
    let file = SourceFile::new(
        "ffi_bad_arity.nxl",
        "extern \"kernel32.dll\" {\n\
             fn lstrlenA(s: String) -> Int;\n\
         }\n\
         print(lstrlenA());\n",
    );
    let error = nxl_compiler::compile_to_nir(&file).expect_err("arity must match");
    assert_eq!(error.code, "NXL2009");
    assert!(
        error.message.contains("expects 1 argument(s), found 0"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_unsupported_type_is_a_compile_error() {
    let file = SourceFile::new(
        "ffi_bad_type.nxl",
        "extern \"kernel32.dll\" {\n\
             fn weird(x: Array) -> Int;\n\
         }\n",
    );
    let error = nxl_compiler::compile_to_nir(&file).expect_err("only scalars are supported");
    assert_eq!(error.code, "NXL2001");
    assert!(
        error.message.contains("unsupported C type"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_excessive_arity_is_a_compile_error() {
    let file = SourceFile::new(
        "ffi_too_many.nxl",
        "extern \"kernel32.dll\" {\n\
             fn five(a: Int, b: Int, c: Int, d: Int, e: Int) -> Int;\n\
         }\n",
    );
    let error = nxl_compiler::compile_to_nir(&file).expect_err("arity is capped at four");
    assert_eq!(error.code, "NXL2001");
    assert!(
        error.message.contains("at most 4"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_duplicate_declaration_is_a_compile_error() {
    let file = SourceFile::new(
        "ffi_dup.nxl",
        "extern \"kernel32.dll\" {\n\
             fn thing() -> Int;\n\
         }\n\
         extern \"kernel32.dll\" {\n\
             fn thing() -> Int;\n\
         }\n",
    );
    let error = nxl_compiler::compile_to_nir(&file).expect_err("extern names must be unique");
    assert_eq!(error.code, "NXL2005");
    assert!(
        error.message.contains("duplicate extern function 'thing'"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_call_is_rejected_by_the_js_backend() {
    let source = SourceFile::new(
        "ffi_js.nxl",
        "extern \"kernel32.dll\" {\n\
             fn GetTickCount64() -> Int;\n\
         }\n\
         print(GetTickCount64());\n",
    );
    let error = nxl_compiler::backend::compile_js(&source)
        .expect_err("the JS backend cannot link native libraries");
    assert_eq!(error.code, "NXL2007");
    assert!(
        error.message.contains("C-ABI extern call"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn extern_call_is_rejected_by_the_rust_backend() {
    let source = SourceFile::new(
        "ffi_rust.nxl",
        "extern \"kernel32.dll\" {\n\
             fn GetTickCount64() -> Int;\n\
         }\n\
         print(GetTickCount64());\n",
    );
    let error = nxl_compiler::backend::compile_native(&source)
        .expect_err("the Rust backend cannot link native libraries");
    assert_eq!(error.code, "NXL2007");
}

#[test]
fn extern_void_return_never_prints_a_value() {
    // ZeroMemory is (PVOID, SIZE_T) -> void: a void return lowers to Unit.
    #[cfg(windows)]
    let source_text = "extern \"kernel32.dll\" {\n\
         fn ZeroMemory(destination: Int, length: Int);\n\
     }\n\
     print(\"ok\");\n";
    #[cfg(not(windows))]
    let source_text = "extern \"libc.so.6\" {\n\
         fn sync();\n\
     }\n\
     print(\"ok\");\n";
    let output = run("ffi_void.nxl", source_text);
    assert_eq!(output, vec!["ok"]);
}

#[test]
fn extern_alias_maps_nxl_name_to_symbol() {
    #[cfg(windows)]
    let source_text = "extern \"kernel32.dll\" {\n\
         fn ticks() -> Int as \"GetTickCount64\";\n\
     }\n\
     print(ticks() >= 0);\n";
    #[cfg(target_os = "linux")]
    let source_text = "extern \"libc.so.6\" {\n\
         fn parent() -> Int as \"getppid\";\n\
     }\n\
     print(parent() >= 0);\n";
    #[cfg(target_os = "macos")]
    let source_text = "extern \"libSystem.B.dylib\" {\n\
         fn proc() -> Int as \"getpid\";\n\
     }\n\
     print(proc() > 0);\n";
    let output = run("ffi_alias.nxl", source_text);
    assert_eq!(output, vec!["true"]);
}
