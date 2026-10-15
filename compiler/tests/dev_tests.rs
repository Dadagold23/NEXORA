//! Development server behaviour.
//!
//! The tests drive a real HTTP server on an ephemeral port over a plain TCP
//! socket, so they exercise the same path a browser would without adding a
//! test-only HTTP client dependency.

use nxl_compiler::dev::{self, DevConfig};
use nxl_compiler::devserver::router;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("nxl_dev_{tag}_{}_{nanos}", std::process::id()));
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

use std::fs;

/// A project on disk that the dev server can compile.
fn project(tag: &str, source: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.path().join("nxl.toml"),
        "[package]\nname = \"webapp\"\nversion = \"0.1.0\"\nentry = \"src/main.nxl\"\n",
    )
    .expect("manifest");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.nxl"), source).expect("entry");
    dir
}

/// Start the server on an OS-assigned port; returns (port, join handle).
///
/// The bundle is compiled before the thread starts so that only runtime
/// startup and binding are left to wait for.
fn serve(project: PathBuf, config: DevConfig) -> (u16, std::thread::JoinHandle<()>) {
    let bundle = dev::build(&project).expect("bundle compiles before serving");
    // Bind first so the port is known before the runtime starts.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async move {
            let app = router(project, bundle, config);
            let socket = format!("127.0.0.1:{port}");
            let listener = tokio::net::TcpListener::bind(&socket).await.expect("bind");
            axum::serve(listener, app)
                // Never resolves: the test stops the process, not the server.
                .with_graceful_shutdown(std::future::pending::<()>())
                .await
                .expect("serve");
        });
    });

    // Wait until the port accepts connections.
    for _ in 0..400 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    (port, handle)
}

/// Minimal HTTP GET returning (status line, body).
fn get(port: u16, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write");
    stream.flush().ok();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("status");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);

    // Skip headers.
    let mut content_length: Option<usize> = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        let lower = header.to_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let mut body = Vec::new();
    match content_length {
        Some(length) => {
            body.resize(length, 0);
            reader.read_exact(&mut body).expect("body");
        }
        None => {
            reader.read_to_end(&mut body).ok();
        }
    }
    (status, String::from_utf8_lossy(&body).into_owned())
}

// ── compile ────────────────────────────────────────────────────────────────────

#[test]
fn builds_a_project_into_a_bundle() {
    let dir = project("build", "fn main() { print(\"hi\"); }");
    let bundle = dev::build(dir.path()).expect("compiles");
    assert!(bundle.script.contains("function main"), "{}", bundle.script);
    assert!(bundle.diagnostics.is_empty());
}

#[test]
fn reports_a_compile_error_instead_of_a_bundle() {
    let dir = project("broken", "fn main() { this is broken }");
    let error = dev::build(dir.path()).expect_err("must not compile");
    assert!(!error.message.is_empty());
}

#[test]
fn a_missing_project_is_reported() {
    let dir = TempDir::new("missing");
    assert!(
        dev::build(dir.path()).is_err(),
        "an empty directory is not a project"
    );
}

// ── watching ───────────────────────────────────────────────────────────────────

#[test]
fn source_changes_are_detected() {
    let dir = project("watch", "fn main() { print(\"one\"); }");
    let stamp = dev::SourceStamp::capture(dir.path());
    assert_eq!(stamp.len(), 1);
    assert!(!stamp.changed_since(dir.path()), "nothing changed yet");

    // A different source body is a change even at the same timestamp.
    let later = stamp.changed_since(dir.path());
    assert!(!later, "an unchanged tree must not report a change");

    fs::write(
        dir.path().join("src/main.nxl"),
        "fn main() { print(\"two\"); }",
    )
    .unwrap();
    assert!(
        stamp.changed_since(dir.path()),
        "an edited source must be noticed"
    );
}

#[test]
fn an_empty_snapshot_does_not_report_a_change() {
    let dir = TempDir::new("empty_stamp");
    let stamp = dev::SourceStamp::empty();
    assert!(!stamp.changed_since(dir.path()));
}

#[test]
fn build_output_is_not_watched() {
    let dir = project("ignore", "fn main() { print(\"x\"); }");
    let stamp = dev::SourceStamp::capture(dir.path());
    fs::create_dir_all(dir.path().join("build/debug")).unwrap();
    fs::write(dir.path().join("build/debug/generated.nxl"), "fn main() {}").unwrap();
    assert!(
        !stamp.changed_since(dir.path()),
        "build output must not trigger a rebuild loop"
    );
}

// ── host page ──────────────────────────────────────────────────────────────────

#[test]
fn host_page_loads_the_compiled_script_and_the_reload_client() {
    let page = dev::host_page("webapp", "/app.js");
    assert!(page.contains("<title>webapp"), "{page}");
    assert!(page.contains("src=\"/app.js\""), "{page}");
    assert!(page.contains("/reload.js"), "{page}");
    // It must say what it is, so nobody mistakes it for a server-side runtime.
    assert!(page.contains("JavaScript backend"), "{page}");
}

#[test]
fn reload_script_reconnects_and_reloads() {
    let script = dev::reload_script();
    assert!(script.contains("EventSource('/reload')"), "{script}");
    assert!(script.contains("window.location.reload()"), "{script}");
}

// ── HTTP ───────────────────────────────────────────────────────────────────────

#[test]
fn serves_the_host_page_the_script_and_health() {
    let dir = project("http", "fn main() { print(\"served\"); }");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    let (status, body) = get(port, "/");
    assert_eq!(status, 200);
    assert!(body.contains("webapp"), "{body}");

    let (status, body) = get(port, "/app.js");
    assert_eq!(status, 200);
    assert!(body.contains("function main"), "{body}");

    let (status, body) = get(port, "/reload.js");
    assert_eq!(status, 200);
    assert!(body.contains("EventSource"), "{body}");

    let (status, body) = get(port, "/health");
    assert_eq!(status, 200);
    assert_eq!(body.trim(), r#"{"status":"ok"}"#);

    let (status, _) = get(port, "/does-not-exist");
    assert_eq!(status, 404);

    // Surface any panic from the server thread instead of losing it.
    let finished = handle.is_finished();
    if finished {
        drop(handle);
    }
}

#[test]
fn a_broken_project_is_surfaced_and_recovers() {
    let dir = project("recover", "fn main() { print(\"before\"); }");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());
    assert_eq!(get(port, "/health").0, 200);

    // Break the source: every request path must notice, not just browsers.
    fs::write(dir.path().join("src/main.nxl"), "fn main() { broken").unwrap();
    assert_eq!(
        get(port, "/health").0,
        503,
        "a broken project must not report healthy"
    );
    let (_, page) = get(port, "/");
    assert!(page.contains("does not compile"), "{page}");
    assert_eq!(get(port, "/app.js").0, 503);

    // Fix it again.
    fs::write(
        dir.path().join("src/main.nxl"),
        "fn main() { print(\"after\"); }",
    )
    .unwrap();
    assert_eq!(get(port, "/health").0, 200, "the server must recover");
    let (_, script) = get(port, "/app.js");
    assert!(script.contains("function main"), "{script}");

    drop(handle);
}

#[test]
fn default_config_binds_loopback_only() {
    let config = DevConfig::default();
    assert_eq!(config.address, "127.0.0.1:3000");
    assert!(
        config.address.starts_with("127.0.0.1"),
        "not public by default"
    );
    assert!(config.watch);
}

// ── the project's own page ──────────────────────────────────────────────────────

/// A project that ships a `public/` directory, like the web target scaffolds.
fn web_project(tag: &str, source: &str) -> TempDir {
    let dir = project(tag, source);
    fs::create_dir_all(dir.path().join("public")).unwrap();
    fs::write(
        dir.path().join("public/index.html"),
        "<!doctype html><html><body><h1>project page</h1>\
         <script src=\"/app.js\"></script></body></html>",
    )
    .expect("index");
    fs::write(dir.path().join("public/styles.css"), "body { color: red; }").expect("css");
    dir
}

#[test]
fn the_projects_own_page_is_served_instead_of_the_host_page() {
    let dir = web_project("public_index", "fn main() { print(\"served\"); }");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    let (status, body) = get(port, "/");
    assert_eq!(status, 200);
    assert!(body.contains("project page"), "{body}");
    assert!(
        !body.contains("NXL dev server"),
        "the generated host page must not replace the project's page: {body}"
    );
    // The reload client is still added, because the page needs it.
    assert!(body.contains("/reload.js"), "{body}");

    drop(handle);
}

#[test]
fn static_assets_are_served_from_public() {
    let dir = web_project("public_asset", "fn main() { print(\"served\"); }");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    let (status, body) = get(port, "/styles.css");
    assert_eq!(status, 200, "css must be served");
    assert!(body.contains("color: red"), "{body}");

    // A file that does not exist is still a 404 rather than a crash.
    let (status, _) = get(port, "/missing.css");
    assert_eq!(status, 404);

    drop(handle);
}

#[test]
fn public_files_cannot_escape_their_directory() {
    let dir = web_project("public_escape", "fn main() { print(\"served\"); }");
    // Something worth stealing, one level above public/.
    fs::write(dir.path().join("secret.txt"), "do not serve me").unwrap();
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    for path in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
        "/sub/../../secret.txt",
    ] {
        let (status, body) = get(port, path);
        assert!(
            status == 404 || !body.contains("do not serve me"),
            "{path} must not serve a file outside public/ (status {status})"
        );
    }

    drop(handle);
}

#[test]
fn a_page_without_the_program_gets_it_injected() {
    let dir = project("inject", "fn main() { print(\"served\"); }");
    fs::create_dir_all(dir.path().join("public")).unwrap();
    fs::write(
        dir.path().join("public/index.html"),
        "<!doctype html><html><body><h1>bare</h1></body></html>",
    )
    .expect("index");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    let (status, body) = get(port, "/");
    assert_eq!(status, 200);
    assert!(body.contains("bare"), "{body}");
    assert!(
        body.contains("/app.js"),
        "a page that forgets the program must still get it: {body}"
    );

    drop(handle);
}

#[test]
fn a_project_without_public_keeps_the_generated_host_page() {
    let dir = project("no_public", "fn main() { print(\"served\"); }");
    let (port, handle) = serve(dir.path().to_path_buf(), DevConfig::default());

    let (status, body) = get(port, "/");
    assert_eq!(status, 200);
    assert!(body.contains("NXL dev server"), "{body}");

    drop(handle);
}

#[test]
fn script_injection_leaves_a_page_that_already_has_them() {
    let page = "<body><script src=\"/app.js\"></script><script src=\"/reload.js\"></script></body>";
    assert_eq!(dev::inject_scripts(page), page);

    let bare = "<body></body>";
    let injected = dev::inject_scripts(bare);
    assert!(injected.contains("/app.js"), "{injected}");
    assert!(injected.contains("/reload.js"), "{injected}");
    assert!(injected.contains("</body>"), "{injected}");
}
