//! End-to-end smoke tests for the real `nxlc` binary: generate a project the
//! way a developer would, build it, run its tests, and check that overwriting
//! and malformed manifests fail loudly. No mocks — every leg runs the
//! compiled executable through `std::process::Command`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nxlc")
}

fn temp_workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nxl_smoke_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create smoke workspace");
    dir
}

fn run(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("nxlc process spawns")
}

fn combined(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_ok(output: &std::process::Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed:\n{}",
        combined(output)
    );
}

fn assert_fails_with(output: &std::process::Output, what: &str, needle: &str) {
    assert!(
        !output.status.success(),
        "{what} unexpectedly succeeded:\n{}",
        combined(output)
    );
    assert!(
        combined(output).contains(needle),
        "{what} failed without '{needle}':\n{}",
        combined(output)
    );
}

#[test]
fn cli_new_web_generates_a_buildable_testable_project() {
    let workspace = temp_workspace("web");

    // Generate.
    let new = run(&workspace, &["new", "web", "smokeapp"]);
    assert_ok(&new, "nxlc new web");

    let project = workspace.join("smokeapp");
    for file in [
        "nxl.toml",
        "src/main.nxl",
        "src/app/routes.nxl",
        "pages/home.nxlhtml",
        "layouts/main.nxlhtml",
        "tests/basic.nxl",
    ] {
        assert!(
            project.join(file).exists(),
            "generated project lacks {file}"
        );
    }
    // The manifest must carry the real project name, not the template placeholder.
    let manifest =
        std::fs::read_to_string(project.join("nxl.toml")).expect("read generated manifest");
    assert!(manifest.contains("name = \"smokeapp\""), "{manifest}");
    assert!(!manifest.contains("PROJECT_NAME"), "{manifest}");

    // Generating again into the same name must refuse to overwrite.
    let again = run(&workspace, &["new", "web", "smokeapp"]);
    assert_fails_with(&again, "second nxlc new web", "already exists");

    // Build compiles the entry point to a real artifact.
    let build = run(&project, &["build"]);
    assert_ok(&build, "nxlc build");
    let debug_dir = project.join("build").join("debug");
    let has_artifact = std::fs::read_dir(&debug_dir)
        .map(|entries| entries.filter_map(Result::ok).next().is_some())
        .unwrap_or(false);
    assert!(
        has_artifact,
        "build succeeded but {} holds no artifact",
        debug_dir.display()
    );

    // The generated project's own tests pass.
    let test = run(&project, &["test"]);
    assert_ok(&test, "nxlc test");

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn cli_new_console_generates_a_buildable_project() {
    let workspace = temp_workspace("console");

    let new = run(&workspace, &["new", "console", "plainapp"]);
    assert_ok(&new, "nxlc new console");
    let project = workspace.join("plainapp");
    assert!(project.join("nxl.toml").exists());
    assert!(project.join("src/main.nxl").exists());

    let build = run(&project, &["build"]);
    assert_ok(&build, "nxlc build (console)");

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn cli_build_refuses_a_manifest_with_a_typo() {
    // A typo'd [package] key used to be silently dropped; it must fail the
    // real `build` command with the NXL12012 diagnostic.
    let workspace = temp_workspace("typo");
    let new = run(&workspace, &["new", "console", "typoapp"]);
    assert_ok(&new, "nxlc new console");

    let project = workspace.join("typoapp");
    let manifest_path = project.join("nxl.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let broken = manifest.replace("entry =", "targt = \"vm\"\nentry =");
    std::fs::write(&manifest_path, broken).expect("write broken manifest");

    let build = run(&project, &["build"]);
    assert_fails_with(&build, "nxlc build with a typo'd key", "NXL12012");

    let _ = std::fs::remove_dir_all(&workspace);
}
