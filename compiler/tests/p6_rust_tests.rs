fn rust_available() -> bool {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Compile generated Rust with rustc and run it; returns its stdout.
fn run_native(program: &str, name: &str) -> String {
    let source = nxl_compiler::source::SourceFile::new(name, program);
    let artifact = nxl_compiler::backend::compile_native(&source).expect("native emit");
    let dir = std::env::temp_dir().join(format!("nxl_p6_{}", name.replace('.', "_")));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let source_path = dir.join("main.rs");
    let binary_path = dir.join("main.exe");
    std::fs::write(&source_path, &artifact.source).expect("write");
    let output = std::process::Command::new("rustc")
        .arg(&source_path)
        .arg("-o")
        .arg(&binary_path)
        .output()
        .expect("rustc runs");
    assert!(
        output.status.success(),
        "generated Rust must compile:\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        artifact.source
    );
    let run = std::process::Command::new(&binary_path)
        .output()
        .expect("binary runs");
    String::from_utf8_lossy(&run.stdout).to_string()
}

#[test]
fn native_try_catch_round_trip() {
    if !rust_available() {
        return;
    }
    let stdout = run_native(
        "try { throw \"boom\"; } catch e { print(e); }",
        "native_try_catch",
    );
    assert_eq!(stdout.trim(), "boom");
}

#[test]
fn native_try_without_throw_runs_body() {
    if !rust_available() {
        return;
    }
    let stdout = run_native(
        "try { print(\"body\"); } catch e { print(e); } print(\"after\");",
        "native_try_body",
    );
    assert_eq!(stdout.lines().collect::<Vec<_>>(), vec!["body", "after"]);
}

#[test]
fn native_try_finally_runs_once_on_both_paths() {
    if !rust_available() {
        return;
    }
    let clean = run_native(
        "try { print(\"body\"); } finally { print(\"finally\"); }",
        "native_finally_ok",
    );
    assert_eq!(clean.lines().collect::<Vec<_>>(), vec!["body", "finally"]);

    let thrown = run_native(
        "try { throw \"boom\"; } catch e { print(e); } finally { print(\"finally\"); }",
        "native_finally_err",
    );
    assert_eq!(thrown.lines().collect::<Vec<_>>(), vec!["boom", "finally"]);
}

#[test]
fn native_finally_reraises_uncaught() {
    if !rust_available() {
        return;
    }
    let source = nxl_compiler::source::SourceFile::new(
        "native_reraise.nxl",
        "try { throw \"boom\"; } finally { print(\"cleanup\"); }",
    );
    let artifact = nxl_compiler::backend::compile_native(&source).expect("emit");
    assert!(
        artifact.source.contains("resume_unwind"),
        "an uncaught error must still propagate: {}",
        artifact.source
    );
}

#[test]
fn native_rejects_unsupported_control_flow_loudly() {
    for (name, program) in [
        (
            "native_branch",
            "let flag = true; if flag { print(1); } else { print(2); }",
        ),
        ("native_while", "let mut i = 0; while i < 3 { i = i + 1; }"),
        ("native_forin", "for v in [1, 2] { print(v); }"),
        (
            "native_jump",
            "let mut i = 0; while true { if i > 2 { break; } i = i + 1; }",
        ),
    ] {
        let source = nxl_compiler::source::SourceFile::new(name, program);
        let err = nxl_compiler::backend::compile_native(&source)
            .expect_err("the Rust backend must refuse what it cannot emit");
        assert_eq!(err.code, "NXL2007", "unexpected code for {name}");
        assert!(
            err.message.contains("JavaScript backend") || err.message.contains("VM"),
            "the error must point at a working target: {err:?}"
        );
    }
}

#[test]
fn native_rejects_annotated_catch_loudly() {
    let source = nxl_compiler::source::SourceFile::new(
        "native_typed_catch.nxl",
        "try { throw \"boom\"; } catch e: String { print(e); }",
    );
    let err = nxl_compiler::backend::compile_native(&source)
        .expect_err("annotated catches must be refused");
    assert_eq!(err.code, "NXL2007");
}
