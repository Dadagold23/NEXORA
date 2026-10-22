//! End-to-end smoke tests for the real `nxlc` binary: generate a project the
//! way a developer would, build it, run its tests, serve it, and check that
//! overwriting and malformed manifests fail loudly. No mocks — every leg runs
//! the compiled executable through `std::process::Command`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

// ── the dev server, end to end ────────────────────────────────────────────────

/// A raw HTTP exchange over a fresh connection, so no client dependency is
/// needed. Returns the raw response — status line, headers and body.
fn http(addr: &str, method: &str, target: &str, body: Option<&str>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(addr).expect("connect to the dev server");
    let payload = body.unwrap_or("");
    let head = format!(
        "{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes()).expect("send request");
    stream.write_all(payload.as_bytes()).expect("send body");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read response");
    raw
}

fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

#[test]
fn cli_serve_answers_the_generated_routes() {
    // The server a developer actually starts: the real `nxlc serve` process,
    // reading `nxl.toml`, rendering `pages/`, and answering over HTTP. The
    // in-process tests in web_tests.rs cover rendering; this covers the wiring
    // between the binary, the manifest and the socket.
    let workspace = temp_workspace("serve");
    let new = run(&workspace, &["new", "web", "serveapp"]);
    assert_ok(&new, "nxlc new web");
    let project = workspace.join("serveapp");

    // The generated manifest pins port 8080, which another process may hold.
    // A free port keeps this leg from flaking and proves `port` is honoured.
    let port = pick_free_port();
    let manifest_path = project.join("nxl.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read manifest");
    std::fs::write(
        &manifest_path,
        manifest.replace("port = 8080", &format!("port = {port}")),
    )
    .expect("write manifest with a free port");

    let mut server = Command::new(bin())
        .args(["serve"])
        .current_dir(&project)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start nxlc serve");
    let addr = format!("127.0.0.1:{port}");

    // Wait for the socket, not a fixed sleep: the build step inside `serve`
    // can take a moment.
    let mut up = false;
    for _ in 0..80 {
        if std::net::TcpStream::connect(&addr).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert!(up, "nxlc serve never listened on {addr}");

    let home = http(&addr, "GET", "/", None);
    assert!(home.starts_with("HTTP/1.1 200"), "{home}");
    assert!(home.contains("<title>serveapp</title>"), "{home}");
    assert!(home.contains("Welcome to serveapp"), "{home}");
    assert!(home.contains("x-content-type-options: nosniff"), "{home}");

    let parameterized = http(&addr, "GET", "/posts/42", None);
    assert!(parameterized.starts_with("HTTP/1.1 200"), "{parameterized}");
    assert!(parameterized.contains("42"), "{parameterized}");

    // An incomplete submission is refused with the validation message.
    let rejected = http(&addr, "POST", "/contact", Some("name=Ada"));
    assert!(rejected.starts_with("HTTP/1.1 422"), "{rejected}");
    assert!(rejected.contains("email"), "{rejected}");

    // A complete one renders the page's success state.
    let accepted = http(
        &addr,
        "POST",
        "/contact",
        Some("name=Ada&email=ada%40example.com&message=Hello"),
    );
    assert!(accepted.starts_with("HTTP/1.1 200"), "{accepted}");
    assert!(accepted.contains("class=\"success\""), "{accepted}");

    // The 404 tells the browser which path missed and offers navigation; it is
    // a real page, not a bare <h1>.
    let missing = http(&addr, "GET", "/definitely-not-a-page", None);
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert!(
        missing.contains("<a href=\"/\">Home</a>"),
        "the 404 page must offer real navigation:\n{missing}"
    );

    // A path segment is echoed back escaped, never as markup.
    let escaped = http(&addr, "GET", "/posts/%3Cb%3Ex%3C%2Fb%3E", None);
    assert!(escaped.starts_with("HTTP/1.1 200"), "{escaped}");
    assert!(!escaped.contains("<b>x</b>"), "a tag survived: {escaped}");
    assert!(escaped.contains("&lt;b&gt;x&lt;/b&gt;"), "{escaped}");

    server.kill().expect("stop nxlc serve");
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&workspace);
}
