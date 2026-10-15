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
    repo_root().join("templates/console")
}

fn repo_web_template_dir() -> PathBuf {
    repo_root().join("templates/web")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ has a parent")
        .to_path_buf()
}

// ── command surface ─────────────────────────────────────────────────────────────

#[test]
fn new_accepts_a_target_in_every_documented_form() {
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
        vec!["new", "--target", "console", "hello"],
        vec!["new", "--target=console", "hello"],
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
fn new_accepts_the_web_target_in_every_documented_form() {
    for form in [
        vec!["new", "web", "site"],
        vec!["new", "--target", "web", "site"],
        vec!["new", "--target=web", "site"],
        vec!["new", "--template", "web", "site"],
    ] {
        assert_eq!(
            parse_args(&args(&form)).unwrap(),
            Command::New {
                name: "site".into(),
                template: Some("web".into())
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
fn dev_parses() {
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
fn unknown_targets_are_refused_with_an_explanation() {
    let dir = TempDir::new("unknown_target");
    let err = cmd_new_from_template_in(dir.path(), "site", "mobile")
        .expect_err("an unknown target must not invent a project");
    assert_eq!(err.code, "NXL12008", "{err:?}");
    assert!(err.message.contains("unknown target"), "{err:?}");
    assert!(err.message.contains("console"), "{err:?}");
    assert!(err.message.contains("web"), "{err:?}");
    assert!(
        !dir.path().join("site").exists(),
        "a refused target must not create anything"
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
fn both_targets_are_advertised() {
    assert_eq!(TEMPLATES, ["console", "web"]);
}

#[test]
fn a_mistyped_target_is_rejected_instead_of_silently_dropped() {
    // `new <name>` is valid, so `new webb app` used to scaffold a console
    // project called "webb" and quietly ignore "app".
    let err = parse_args(&args(&["new", "webb", "app"]))
        .expect_err("a mistyped target must not be accepted");
    assert!(
        err.message.contains("webb"),
        "the error should name what was typed: {err:?}"
    );
    assert!(err.message.contains("web"), "{err:?}");

    let dir = TempDir::new("mistyped");
    let err = cmd_new_from_template_in(dir.path(), "app", "webb")
        .expect_err("the same mistake at the command layer");
    assert_eq!(err.code, "NXL12008", "{err:?}");
    assert!(
        !dir.path().join("app").exists(),
        "nothing may be created for a refused target"
    );
}

// ── the web target ─────────────────────────────────────────────────────────────

#[test]
fn web_template_creates_a_full_project_worktree() {
    let dir = TempDir::new("web");
    let message = cmd_new_from_template_in(dir.path(), "site", "web").expect("web template");
    assert!(message.contains("site"), "{message}");
    assert!(message.contains("nexora dev"), "{message}");

    let project = dir.path().join("site");
    for file in [
        "nxl.toml",
        "README.md",
        ".gitignore",
        "src/main.nxl",
        "src/app/config.nxl",
        "src/app/routes.nxl",
        "src/app/state.nxl",
        "public/index.html",
        "public/styles.css",
        "public/bootstrap.js",
        "public/favicon.svg",
        "tests/basic.nxl",
    ] {
        assert!(
            project.join(file).exists(),
            "the web template must create {file}"
        );
    }

    // The landing page carries a splash screen and loads the program.
    let index = fs::read_to_string(project.join("public/index.html")).unwrap();
    assert!(
        index.contains("id=\"splash\""),
        "the page needs a splash screen"
    );
    assert!(index.contains("/app.js"), "the page must load the program");
    assert!(
        index.contains("/bootstrap.js"),
        "the bridge must load first"
    );
    assert!(
        index.contains("/styles.css"),
        "the page must load its stylesheet"
    );

    // The manifest names the project and targets JavaScript.
    let manifest = fs::read_to_string(project.join("nxl.toml")).unwrap();
    assert!(manifest.contains("name = \"site\""), "{manifest}");
    assert!(manifest.contains("entry = \"src/main.nxl\""), "{manifest}");

    // Nothing is left dangling: the program prints the sentinel the page waits
    // for, and every module it imports exists.
    let main = fs::read_to_string(project.join("src/main.nxl")).unwrap();
    assert!(main.contains("READY"), "the boot log must end with READY");
    for module in ["config", "routes", "state"] {
        assert!(
            main.contains(&format!("app.{module}")),
            "main imports {module}"
        );
        assert!(project.join(format!("src/app/{module}.nxl")).exists());
    }
}

#[test]
fn web_template_project_compiles_and_runs() {
    let dir = TempDir::new("web_run");
    cmd_new_from_template_in(dir.path(), "site", "web").unwrap();
    let project = dir.path().join("site");

    // The whole program, modules and all, runs on the VM.
    let run = nxl_compiler::project::compile_project(&project.join("src/main.nxl"))
        .expect("the generated project compiles");
    let output = nxl_compiler::vm::execute(&run)
        .expect("the generated project runs")
        .output
        .join("\n");
    assert!(output.contains("booting site"), "{output}");
    assert!(output.contains("READY"), "{output}");

    // And it emits JavaScript for the dev server to serve.
    let js = nxl_compiler::backend::compile_js_module(&run).expect("JavaScript is emitted");
    assert!(
        js.source.contains("console.log"),
        "print must reach the page"
    );

    // The self-check passes.
    let report = nxl_compiler::cli::cmd_test(&project, None).expect("tests run");
    assert_eq!(report.failures, Vec::<String>::new(), "{report:?}");
    assert_eq!(report.passed, report.total, "{report:?}");
}

#[test]
fn web_template_scaffolding_locks_dependencies() {
    let dir = TempDir::new("web_lock");
    cmd_new_from_template_in(dir.path(), "site", "web").unwrap();
    // The manifest is resolved as part of scaffolding, so the project is ready
    // to run rather than needing a second command first.
    assert!(
        dir.path().join("site/nxl.lock").exists(),
        "scaffolding must leave a lock file behind"
    );
}

#[test]
fn the_scaffolded_page_is_wired_to_the_program() {
    // Compiling and running the project is not the same as proving the page
    // shows anything: the landing page depends on bootstrap.js turning the
    // program's output into DOM changes, and a regression there would leave a
    // blank page that every other test still calls healthy.
    if !nxl_compiler::runtime::Runtime::node_available() {
        return;
    }
    let dir = TempDir::new("web_page");
    cmd_new_from_template_in(dir.path(), "site", "web").unwrap();
    let project = dir.path().join("site");

    let module = nxl_compiler::project::compile_project(&project.join("src/main.nxl"))
        .expect("the project compiles");
    let artifact =
        nxl_compiler::backend::compile_js_module(&module).expect("JavaScript is emitted");
    let bundle = project.join("build/debug/site.js");
    fs::create_dir_all(bundle.parent().unwrap()).unwrap();
    fs::write(&bundle, &artifact.source).expect("write bundle");

    let script = repo_root().join("scripts/verify-web-page.js");
    if !script.exists() {
        return; // not running from a checkout
    }
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg(&project)
        .output()
        .expect("node runs");
    assert!(
        output.status.success(),
        "the scaffolded page must be wired to its program:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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

#[test]
fn embedded_web_template_matches_the_repository_copy() {
    let repo = repo_web_template_dir();
    if !repo.exists() {
        return; // not running from a checkout
    }
    for (relative, embedded) in nxl_compiler::cli::WEB_TEMPLATE {
        let on_disk = fs::read_to_string(repo.join(relative))
            .unwrap_or_else(|e| panic!("templates/web/{relative}: {e}"));
        let normalise = |text: String| text.replace("\r\n", "\n").trim_end().to_owned();
        assert_eq!(
            normalise(on_disk),
            normalise(embedded.to_owned()),
            "templates/web/{relative} and the embedded copy have drifted apart"
        );
    }
}

#[test]
fn every_file_under_the_web_template_is_embedded() {
    // A file added to templates/web/ but forgotten in WEB_TEMPLATE would be
    // missing from projects created by an installed compiler, while still
    // working in a checkout. Compare the two sets both ways.
    let repo = repo_web_template_dir();
    if !repo.exists() {
        return;
    }
    let mut on_disk = Vec::new();
    let mut stack = vec![repo.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read template dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(&repo) {
                on_disk.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    on_disk.sort();
    let mut embedded = nxl_compiler::cli::WEB_TEMPLATE
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect::<Vec<_>>();
    embedded.sort();
    assert_eq!(
        on_disk, embedded,
        "templates/web and WEB_TEMPLATE must list the same files"
    );
}
