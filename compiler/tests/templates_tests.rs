//! Project templates and the `nexora` command surface (R07/R08).

use nxl_compiler::cli::{
    cmd_new_from_template_in, parse_args, Command, CONSOLE_TEMPLATE, TEMPLATES,
};
use nxl_compiler::source::SourceFile;
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("nxl_tpl_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

fn repo_template_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ has a parent")
        .join("templates/console")
}

// ── command surface ─────────────────────────────────────────────────────────────

#[test]
fn new_accepts_a_template_in_every_documented_form() {
    assert_eq!(
        parse_args(&args(&["new", "hello"])).unwrap(),
        Command::New {
            name: "hello".into(),
            template: None
        }
    );
    for form in [
        vec!["new", "console", "hello"],
        vec!["new", "--template", "console", "hello"],
        vec!["new", "--template=console", "hello"],
    ] {
        assert_eq!(
            parse_args(&args(&form)).unwrap(),
            Command::New {
                name: "hello".into(),
                template: Some("console".into())
            },
            "form {form:?} should parse"
        );
    }
}

#[test]
fn new_without_a_name_is_rejected() {
    assert!(parse_args(&args(&["new"])).is_err());
    assert!(parse_args(&args(&["new", "console"])).is_err());
}

#[test]
fn dev_parses_but_is_not_implemented() {
    assert_eq!(parse_args(&args(&["dev"])).unwrap(), Command::Dev);
}

#[test]
fn help_lists_the_project_commands() {
    let help = nxl_compiler::cli::help_text();
    for command in ["new", "run", "build", "check", "dev", "doctor", "test"] {
        assert!(help.contains(command), "help should mention '{command}'");
    }
}

// ── template creation ───────────────────────────────────────────────────────────

#[test]
fn console_template_creates_a_runnable_project() {
    let dir = TempDir::new("console");
    let message =
        cmd_new_from_template_in(dir.path(), "hello-world", "console").expect("template applies");

    assert!(message.contains("hello-world"), "{message}");
    for file in ["nxl.toml", "src/main.nxl", "tests/basic.nxl", ".gitignore"] {
        assert!(
            dir.path().join("hello-world").join(file).exists(),
            "template must create {file}"
        );
    }

    // The generated entry point must run and say what the docs promise.
    let entry = fs::read_to_string(dir.path().join("hello-world/src/main.nxl")).unwrap();
    assert!(
        entry.contains("hello-world"),
        "the project name is substituted"
    );
    let run = nxl_compiler::vm::run(&SourceFile::new("main.nxl", entry)).expect("runs");
    assert_eq!(run.output, vec!["Hello, NXL!"]);

    // The manifest must name the project and point at the entry point.
    let manifest = fs::read_to_string(dir.path().join("hello-world/nxl.toml")).unwrap();
    assert!(manifest.contains("name = \"hello-world\""), "{manifest}");
    assert!(manifest.contains("entry = \"src/main.nxl\""), "{manifest}");
}

#[test]
fn console_template_test_file_runs() {
    let dir = TempDir::new("tests");
    cmd_new_from_template_in(dir.path(), "p", "console").unwrap();
    let text = fs::read_to_string(dir.path().join("p/tests/basic.nxl")).unwrap();
    let run = nxl_compiler::vm::run(&SourceFile::new("basic.nxl", text)).expect("tests run");
    assert_eq!(run.output, vec!["tests/basic.nxl: checks passed"]);
}

#[test]
fn unknown_templates_are_refused_with_an_explanation() {
    let dir = TempDir::new("web");
    let err = cmd_new_from_template_in(dir.path(), "site", "web")
        .expect_err("there is no web toolchain to scaffold for");
    assert_eq!(err.code, "NXL12008", "{err:?}");
    assert!(err.message.contains("no web template"), "{err:?}");
    assert!(err.message.contains("console"), "{err:?}");
    assert!(
        !dir.path().join("site").exists(),
        "a refused template must not create anything"
    );
}

#[test]
fn creating_over_an_existing_directory_is_refused() {
    let dir = TempDir::new("existing");
    fs::create_dir_all(dir.path().join("taken")).unwrap();
    let err = cmd_new_from_template_in(dir.path(), "taken", "console")
        .expect_err("existing directories must not be overwritten");
    assert_eq!(err.code, "NXL12005", "{err:?}");
}

#[test]
fn only_console_is_advertised_as_available() {
    assert_eq!(TEMPLATES, ["console"]);
}

// ── embedded copy stays in sync ─────────────────────────────────────────────────

#[test]
fn embedded_template_matches_the_repository_copy() {
    let repo = repo_template_dir();
    if !repo.exists() {
        return; // not running from a checkout
    }
    for (relative, embedded) in CONSOLE_TEMPLATE {
        let on_disk = fs::read_to_string(repo.join(relative))
            .unwrap_or_else(|e| panic!("templates/console/{relative}: {e}"));
        let normalise = |text: String| text.replace("\r\n", "\n").trim_end().to_owned();
        assert_eq!(
            normalise(on_disk),
            normalise(embedded.to_owned()),
            "templates/console/{relative} and the embedded copy have drifted apart"
        );
    }
}
