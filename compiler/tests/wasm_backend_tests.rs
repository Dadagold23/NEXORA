//! Integration tests for the WebAssembly backend (`wasm_backend`).
//!
//! Every parity test compiles a program to core WASM, runs it under Node's
//! `WebAssembly` engine through the embedded reference host, and compares the
//! output line-for-line against the VM — the authoritative engine. Refusal
//! tests pin the honest NXL2007 boundary: anything beyond Int/Float/Bool
//! scalars fails loudly instead of emitting wrong bytes.

use nxl_compiler::cli::{BuildTarget, ProjectConfig};
use nxl_compiler::source::SourceFile;
use nxl_compiler::wasm_backend::compile_wasm;
use std::fs;

/// The reference host: instantiates the module and provides the `env`
/// imports. Float formatting mirrors Rust's `Display` for the special cases
/// (NaN, ±inf, -0); the documented exponent-formatting divergence stays out
/// of the parity tests.
const HOST: &str = r#"
import { readFileSync } from "node:fs";

let out = "";
let first = true;

function fmtFloat(x) {
    if (Number.isNaN(x)) return "NaN";
    if (x === Infinity) return "inf";
    if (x === -Infinity) return "-inf";
    if (Object.is(x, -0)) return "-0";
    return String(x);
}

const env = {
    print_i64: (x) => { out += (first ? "" : " ") + x.toString(); first = false; },
    print_f64: (x) => { out += (first ? "" : " ") + fmtFloat(x); first = false; },
    print_bool: (x) => { out += (first ? "" : " ") + (x ? "true" : "false"); first = false; },
    nl: () => { out += "\n"; first = true; },
    math_cbrt: (x) => Math.cbrt(x),
    math_exp: (x) => Math.exp(x),
    math_ln: (x) => Math.log(x),
    math_log2: (x) => Math.log2(x),
    math_log10: (x) => Math.log10(x),
    math_sin: (x) => Math.sin(x),
    math_cos: (x) => Math.cos(x),
    math_tan: (x) => Math.tan(x),
    math_asin: (x) => Math.asin(x),
    math_acos: (x) => Math.acos(x),
    math_atan: (x) => Math.atan(x),
    math_atan2: (y, x) => Math.atan2(y, x),
    math_pow: (b, e) => Math.pow(b, e),
};

const bytes = readFileSync(process.argv[2]);
const { instance } = await WebAssembly.instantiate(bytes, { env });
instance.exports._start();
process.stdout.write(out);
"#;

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Compile one program to WASM and run it under Node; returns output lines.
fn run_wasm(name: &str, program: &str) -> Vec<String> {
    let artifact = compile_wasm(&SourceFile::new(name, program))
        .unwrap_or_else(|d| panic!("{name}: WASM compile failed: {d:?}"));
    let base = std::env::temp_dir().join(format!("nxl_wasm_{}_{}", std::process::id(), name));
    let wasm_path = base.with_extension("wasm");
    let glue_path = base.with_extension("mjs");
    fs::write(&wasm_path, &artifact.bytes).expect("write .wasm");
    fs::write(&glue_path, HOST).expect("write host glue");
    let output = std::process::Command::new("node")
        .arg(&glue_path)
        .arg(&wasm_path)
        .output()
        .expect("node runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{name}: node failed: {}\n(artifacts kept for inspection: {})",
        stderr,
        wasm_path.display()
    );
    // Clean up only after a successful run, so a failed module stays on disk.
    let _ = fs::remove_file(&glue_path);
    let _ = fs::remove_file(&wasm_path);
    String::from_utf8(output.stdout)
        .expect("stdout is utf-8")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Assert a program's WASM output matches the VM line-for-line. Returns
/// early when Node.js is unavailable so the suite still runs without it.
fn assert_wasm_parity(name: &str, program: &str, expected: &[&str]) {
    if !node_available() {
        return;
    }
    let vm_result = nxl_compiler::vm::run(&SourceFile::new(name, program)).expect("VM should run");
    let wasm_lines = run_wasm(name, program);
    assert_eq!(vm_result.output, wasm_lines, "VM/WASM mismatch for {name}");
    assert_eq!(vm_result.output, expected, "unexpected output for {name}");
}

/// Assert a program is refused with the honest NXL2007 diagnostic.
fn assert_wasm_refusal(name: &str, program: &str) {
    let result = compile_wasm(&SourceFile::new(name, program));
    let err = result
        .err()
        .unwrap_or_else(|| panic!("{name}: must be refused"));
    assert_eq!(err.code, "NXL2007", "{name}: refusal must be NXL2007");
}

// ── Module shape ──────────────────────────────────────────────────────────────

#[test]
fn wasm_module_starts_with_magic_and_exports_start() {
    let artifact =
        compile_wasm(&SourceFile::new("magic.nxl", "print(1);")).expect("should compile");
    assert_eq!(&artifact.bytes[0..8], b"\0asm\x01\0\0\0", "magic + version");
    assert_eq!(artifact.target, "wasm");
    assert!(
        artifact.bytes.windows(6).any(|w| w == b"_start"),
        "_start must be among the exports"
    );
}

#[test]
fn wasm_output_is_deterministic() {
    let program =
        "fn fib(n: Int) -> Int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\n\
                   for i in 5 { print(fib(i)); }";
    let first = compile_wasm(&SourceFile::new("det.nxl", program))
        .expect("first compile")
        .bytes;
    let second = compile_wasm(&SourceFile::new("det.nxl", program))
        .expect("second compile")
        .bytes;
    assert_eq!(first, second, "recompiles must be byte-identical");
}

// ── VM parity ─────────────────────────────────────────────────────────────────

#[test]
fn wasm_parity_print_and_integer_arithmetic() {
    assert_wasm_parity(
        "int_arith.nxl",
        "print(1 + 2); print(7 / 2); print(7 % 2); print(1000000 * 1000000); print(0 - 9);",
        &["3", "3", "1", "1000000000000", "-9"],
    );
}

#[test]
fn wasm_parity_float_arithmetic() {
    assert_wasm_parity(
        "float_arith.nxl",
        "print(1.5 + 2.25); print(4.0 / 2.0); print(0.1 + 0.2); print(1.5 * 2.0);",
        &["3.75", "2", "0.30000000000000004", "3"],
    );
}

#[test]
fn bitwise_operators_are_rejected_before_the_backend() {
    // NIR lowering has no arm for BitAnd/BitOr/BitXor/shifts, so these
    // programs never reach any backend (same gap as `0..3` ranges).
    // Pinned so the gap is visible rather than silent.
    let result = compile_wasm(&SourceFile::new(
        "bitwise.nxl",
        "print(12 & 10); print(1 << 10);",
    ));
    let err = result
        .err()
        .unwrap_or_else(|| panic!("bitwise must not compile to WASM"));
    assert_eq!(
        err.code, "NXL2001",
        "rejected by NIR lowering, not the backend"
    );
}

#[test]
fn wasm_parity_if_else() {
    assert_wasm_parity(
        "if_else.nxl",
        "let x = 5;\n\
         if x > 3 { print(1); } else { print(2); }\n\
         if x == 5 { print(3); }",
        &["1", "3"],
    );
}

#[test]
fn wasm_parity_while_loop() {
    assert_wasm_parity(
        "while_loop.nxl",
        "let mut i = 0; while i < 3 { print(i); i = i + 1; } print(i);",
        &["0", "1", "2", "3"],
    );
}

#[test]
fn wasm_parity_for_range_loop() {
    assert_wasm_parity(
        "for_range.nxl",
        "for i in 3 { print(i); } print(42);",
        &["0", "1", "2", "42"],
    );
}

#[test]
fn wasm_parity_nested_loops_with_break_and_continue() {
    assert_wasm_parity(
        "nested_loops.nxl",
        "for i in 3 {\n\
             for j in 3 {\n\
                 if j == 1 { continue; }\n\
                 if i == 2 { break; }\n\
                 print(i * 10 + j);\n\
             }\n\
         }",
        &["0", "2", "10", "12"],
    );
}

#[test]
fn wasm_parity_recursive_function() {
    assert_wasm_parity(
        "fib.nxl",
        "fn fib(n: Int) -> Int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\n\
         print(fib(10));",
        &["55"],
    );
}

#[test]
fn wasm_parity_void_function() {
    assert_wasm_parity(
        "void_fn.nxl",
        "fn show(n: Int) { print(n); }\nshow(1); show(2);",
        &["1", "2"],
    );
}

#[test]
fn wasm_parity_bool_logic() {
    assert_wasm_parity(
        "bools.nxl",
        "let a = true; let b = false;\n\
         print(a && b); print(a || b); print(!b); print(1 < 2);",
        &["false", "true", "true", "true"],
    );
}

#[test]
fn wasm_parity_assignment_inside_arms() {
    // A re-`let` of an existing name is rejected by the frontend (NXL2005
    // duplicate declaration), so the legal in-arm mutation is assignment.
    assert_wasm_parity(
        "arm_assign.nxl",
        "let mut x = 1; if true { x = 2; } else { x = 3; } print(x);\n\
         let mut y = 5; while y > 3 { y = y - 1; } print(y);",
        &["2", "3"],
    );
}

#[test]
fn wasm_parity_std_math() {
    assert_wasm_parity(
        "math.nxl",
        "import std.math;\n\
         print(std.math.sqrt(16.0));\n\
         print(std.math.floor(2.7));\n\
         print(std.math.ceil(2.2));\n\
         print(std.math.round(2.5));\n\
         print(std.math.round(-2.5));\n\
         print(std.math.trunc(-1.7));\n\
         print(std.math.is_nan(std.math.sqrt(-1.0)));\n\
         print(std.math.exp(0.0));\n\
         print(std.math.ln(1.0));\n\
         print(std.math.sin(0.0));\n\
         print(std.math.atan2(0.0, 1.0));\n\
         print(std.math.pow_float(2.0, 10.0));\n\
         print(std.math.to_radians(180.0));",
        &[
            "4",
            "2",
            "3",
            "3",
            "-3",
            "-1",
            "true",
            "1",
            "0",
            "0",
            "0",
            "1024",
            "3.141592653589793",
        ],
    );
}

// ── Honest refusals ───────────────────────────────────────────────────────────

#[test]
fn wasm_refuses_strings() {
    assert_wasm_refusal("strings.nxl", "print(\"hi\");");
}

#[test]
fn wasm_refuses_arrays() {
    assert_wasm_refusal("arrays.nxl", "let a = [1, 2, 3]; print(a[0]);");
}

#[test]
fn wasm_refuses_closures() {
    assert_wasm_refusal(
        "closures.nxl",
        "let f = fn (x: Int) -> Int { return x * 2 };\nprint(f(2));",
    );
}

#[test]
fn wasm_refuses_channels() {
    assert_wasm_refusal("channels.nxl", "let c = channel();");
}

#[test]
fn wasm_refuses_spawn_and_await() {
    assert_wasm_refusal(
        "spawn.nxl",
        "fn f() -> Int { return 1; }\nlet t = spawn f();\nprint(await t);",
    );
}

#[test]
fn wasm_refuses_try_catch_and_throw() {
    assert_wasm_refusal(
        "trycatch.nxl",
        "try { throw \"boom\"; } catch e { print(e); }",
    );
}

#[test]
fn wasm_refuses_extern_ffi() {
    #[cfg(windows)]
    let program = "extern \"kernel32.dll\" {\n    fn GetTickCount64() -> Int;\n}\n\
                   print(GetTickCount64() >= 0);\n";
    #[cfg(not(windows))]
    let program = "extern \"libc.so.6\" {\n    fn getppid() -> Int;\n}\n\
                   print(getppid() >= 0);\n";
    assert_wasm_refusal("extern_ffi.nxl", program);
}

#[test]
fn wasm_refuses_vm_only_stdlib() {
    assert_wasm_refusal(
        "sleep.nxl",
        "import std.concurrency;\nstd.concurrency.sleep(1);\n",
    );
}

#[test]
fn wasm_refuses_entry_point_with_parameters() {
    assert_wasm_refusal("entry_params.nxl", "fn main(x: Int) { print(x); }");
}

#[test]
fn wasm_never_emits_type_changing_rebind() {
    // The frontend rejects a re-`let` of an existing name with NXL2005
    // before the backend sees it; the backend's own NXL2007 refusal for a
    // type-changing binding is defense-in-depth behind that. Either way,
    // the shape never becomes WASM.
    let result = compile_wasm(&SourceFile::new(
        "rebind.nxl",
        "let x = 1;\nif true { let x = 1.5; print(x); } else { print(x); }",
    ));
    let err = result
        .err()
        .unwrap_or_else(|| panic!("type-changing rebind must not emit WASM"));
    assert!(
        err.code == "NXL2005" || err.code == "NXL2007",
        "rejected by frontend or backend, got {}",
        err.code
    );
}

// ── CLI wiring ────────────────────────────────────────────────────────────────

#[test]
fn cli_build_wasm_target_emits_wasm_artifact() {
    let tmp = std::env::temp_dir().join(format!(
        "nxl_test_wasm_build_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    fs::create_dir_all(tmp.join("src")).unwrap();
    fs::write(tmp.join("src").join("main.nxl"), "print(1);").unwrap();
    let config = ProjectConfig {
        name: "wasmapp".into(),
        target: BuildTarget::Wasm,
        ..Default::default()
    };
    fs::write(tmp.join("nxl.toml"), config.to_toml()).unwrap();

    let result = nxl_compiler::cli::cmd_build(&tmp, false);
    let mut artifact_path = None;
    let mut magic_ok = false;
    if let Ok(build) = &result {
        if let Some(path) = &build.artifact_path {
            let bytes = fs::read(path).unwrap_or_default();
            magic_ok = bytes.starts_with(b"\0asm\x01\0\0\0");
            artifact_path = Some(path.clone());
        }
    }
    fs::remove_dir_all(&tmp).ok();

    assert!(result.is_ok(), "wasm build failed: {result:?}");
    let path = artifact_path.expect("artifact path");
    assert_eq!(
        path.extension().and_then(|e| e.to_str()),
        Some("wasm"),
        "artifact must be .wasm, got {path:?}"
    );
    assert!(magic_ok, "artifact must start with the WASM magic header");
}

#[test]
fn manifest_target_wasm_parses_and_round_trips() {
    let cfg = ProjectConfig::parse("[package]\nname = \"wasmapp\"\ntarget = \"wasm\"\n")
        .expect("manifest parses");
    assert_eq!(cfg.target, BuildTarget::Wasm);
    let round = cfg.to_toml();
    assert!(
        round.contains("target = \"wasm\""),
        "to_toml must write the wasm target: {round}"
    );
    let reparsed = ProjectConfig::parse(&round).expect("reparse");
    assert_eq!(reparsed.target, BuildTarget::Wasm);
}
