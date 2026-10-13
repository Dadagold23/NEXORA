/// P23 — Fuzzing & property-based testing
///
/// The compiler must never crash on malformed input. These harnesses feed
/// deterministic pseudo-random and mutated inputs through every entry point
/// and assert the absence of panics (a crash in the compiler is a defect).
use std::panic;

use nxl_compiler::cli::parse_args;
use nxl_compiler::pkg::Manifest;
use nxl_compiler::vm::{self, CompiledProgram, BYTECODE_MAGIC};

/// Deterministic LCG so failures are reproducible without external crates.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

const SEEDS: &[&str] = &[
    "fn main() { print(1); }",
    "let x = \"abc\";",
    "struct S { a: Int }",
    "enum E { A, B }",
    "for i in 0..3 { print(i); }",
    "match x { _ => {} }",
    "trait T { fn f(); }",
    "impl T for S {}",
    "actor A { }",
    "async fn go() { await task; }",
];

fn mutate(input: &str, rng: &mut Rng) -> String {
    let bytes = input.as_bytes().to_vec();
    let mut out = String::new();
    for (i, &b) in bytes.iter().enumerate() {
        match rng.next() % 8 {
            0 => out.push(rng.byte() as char),
            1 => {
                if i + 1 < bytes.len() {
                    out.push(bytes[i + 1] as char);
                }
            }
            2 => {}
            3 => out.push('\n'),
            4 => {
                out.push(b as char);
                out.push(b as char);
            }
            _ => out.push(b as char),
        }
    }
    out
}

fn all_inputs() -> Vec<String> {
    let mut rng = Rng(0x12345678);
    let mut out = Vec::new();
    for _ in 0..40 {
        let seed = SEEDS[rng.next() as usize % SEEDS.len()];
        out.push(mutate(seed, &mut rng));
        out.push(SEEDS[rng.next() as usize % SEEDS.len()].to_string());
    }
    out
}

fn must_not_panic<F: FnMut() + panic::UnwindSafe>(name: &str, mut f: F) {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(&mut f));
    if result.is_err() {
        panic!("{name} panicked on fuzzed input");
    }
}

#[test]
fn fuzz_lexer_never_crashes() {
    for input in all_inputs() {
        must_not_panic("lexer", || {
            let sf = nxl_compiler::source::SourceFile::new("fuzz.nxl", input.clone());
            let _ = nxl_compiler::lexer::lex(&sf);
        });
    }
}

#[test]
fn fuzz_parser_never_crashes() {
    for input in all_inputs() {
        must_not_panic("parser", || {
            let sf = nxl_compiler::source::SourceFile::new("fuzz.nxl", input.clone());
            let _ = nxl_compiler::parse_source(&sf);
        });
    }
}

#[test]
fn fuzz_compiler_pipeline_never_crashes() {
    for input in all_inputs() {
        must_not_panic("compile_to_nir", || {
            let sf = nxl_compiler::source::SourceFile::new("fuzz.nxl", input.clone());
            let _ = nxl_compiler::compile_to_nir(&sf);
        });
    }
}

#[test]
fn fuzz_bytecode_decoder_never_crashes() {
    let mut rng = Rng(0xdead_beef);
    for _ in 0..80 {
        let mut data = vec![0u8; (rng.next() as usize) % 512];
        for b in data.iter_mut() {
            *b = rng.byte();
        }
        must_not_panic("bytecode decode", || {
            let _ = CompiledProgram::deserialize(&data);
        });
    }
    for _ in 0..40 {
        let mut data = BYTECODE_MAGIC.to_le_bytes().to_vec();
        for _ in 0..(rng.next() as usize % 256) {
            data.push(rng.byte());
        }
        must_not_panic("bytecode decode magic", || {
            let _ = CompiledProgram::deserialize(&data);
        });
    }
}

#[test]
fn fuzz_manifest_parser_never_crashes() {
    let mut rng = Rng(0xcafe_f00d);
    for _ in 0..80 {
        let mut text = String::new();
        for _ in 0..(rng.next() as usize % 8) {
            text.push(rng.byte() as char);
            text.push('\n');
        }
        must_not_panic("manifest", || {
            let _ = Manifest::parse(&text);
        });
    }
}

#[test]
fn fuzz_cli_args_never_crashes() {
    let tokens = [
        "new",
        "build",
        "--release",
        "db:migrate",
        "make:model",
        "pkg",
        "",
        "--break=",
        "serve",
        "lsp",
        "ide",
        "bridge",
        "platforms",
        "debug",
    ];
    let mut rng = Rng(0xabcd1234);
    for _ in 0..80 {
        let len = (rng.next() as usize) % 5;
        let args: Vec<String> = (0..len)
            .map(|_| tokens[rng.next() as usize % tokens.len()].to_string())
            .collect();
        must_not_panic("cli parse_args", || {
            let _ = parse_args(&args);
        });
    }
}

#[test]
fn bytecode_serialization_roundtrip_deterministic() {
    let module_src = "fn main() { let x = 1; print(x); }";
    let sf = nxl_compiler::source::SourceFile::new("rt.nxl", module_src);
    let m = nxl_compiler::compile_to_nir(&sf).unwrap();
    let p1 = vm::compile(&m).unwrap();
    let p2 = CompiledProgram::deserialize(&p1.serialize()).unwrap();
    assert_eq!(p1, p2);
}

#[test]
fn compile_is_deterministic_on_valid_input() {
    let src = "let x = 3 + 4;\nprint(x);";
    let sf = nxl_compiler::source::SourceFile::new("det.nxl", src);
    let a = nxl_compiler::compile_to_nir(&sf).unwrap();
    let b = nxl_compiler::compile_to_nir(&sf).unwrap();
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}
