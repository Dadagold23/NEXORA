//! Deployment-hardening tests: startup validation, environment separation,
//! required-variable checks, graceful shutdown, and secret redaction in
//! database errors.

use nxl_compiler::cli;
use nxl_compiler::nuf::{self, DeployConfig, WebConfig};
use std::path::Path;

/// A manifest with a production overlay that differs from the base.
const MANIFEST: &str = r#"
[package]
name = "fixture"
version = "0.1.0"

[web]
host = "127.0.0.1"
port = 8080

[env.production]
host = "0.0.0.0"
port = 80

[env.test]
port = 9999

[deploy]
required_env = ["NXL_TEST_REQUIRED_A"]
"#;

fn manifest_dir(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nxl_deploy_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("nxl.toml"), contents).unwrap();
    dir
}

// ── environment separation ───────────────────────────────────────────────────

#[test]
fn production_overlay_replaces_base_web_settings() {
    let cfg = WebConfig::parse_for_env(MANIFEST, "production");
    assert_eq!(cfg.host, "0.0.0.0", "overlay host wins");
    assert_eq!(cfg.port, 80, "overlay port wins");
    assert_eq!(cfg.environment, "production");
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_overlay_applies_only_to_its_environment() {
    let cfg = WebConfig::parse_for_env(MANIFEST, "test");
    assert_eq!(cfg.host, "127.0.0.1", "base host survives");
    assert_eq!(cfg.port, 9999, "test overlay port wins");

    let cfg = WebConfig::parse_for_env(MANIFEST, "development");
    assert_eq!(cfg.port, 8080, "no development overlay: base stands");
}

#[test]
fn plain_parse_applies_no_overlay() {
    let cfg = WebConfig::parse(MANIFEST);
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 8080);
}

// ── startup validation ───────────────────────────────────────────────────────

#[test]
fn malformed_port_is_an_error_not_a_silent_default() {
    let cfg = WebConfig::parse("[web]\nport = \"eighty\"\n");
    assert_eq!(cfg.port, 8080, "port field keeps the safe default");
    let error = cfg.validate().expect_err("a malformed port must not bind");
    assert!(error.contains("web.port"), "{error}");
    assert!(error.contains("eighty"), "{error}");
}

#[test]
fn port_zero_is_rejected() {
    let cfg = WebConfig {
        port: 0,
        ..Default::default()
    };
    let error = cfg.validate().expect_err("port 0 cannot serve");
    assert!(error.contains("web.port 0"), "{error}");
}

#[test]
fn empty_host_is_rejected() {
    let cfg = WebConfig {
        host: String::new(),
        ..Default::default()
    };
    let error = cfg.validate().expect_err("an empty host cannot bind");
    assert!(error.contains("web.host"), "{error}");
}

#[test]
fn validate_reports_every_problem_at_once() {
    let cfg = WebConfig {
        host: String::new(),
        port: 0,
        ..Default::default()
    };
    let error = cfg.validate().expect_err("both problems must surface");
    assert!(
        error.contains("web.host") && error.contains("web.port"),
        "{error}"
    );
}

// ── required environment variables ───────────────────────────────────────────

#[test]
fn deploy_section_lists_required_variables() {
    let cfg = DeployConfig::parse(MANIFEST);
    assert_eq!(cfg.required_env, vec!["NXL_TEST_REQUIRED_A"]);
}

#[test]
fn missing_reports_unset_variables_by_name_only() {
    let cfg = DeployConfig::parse(MANIFEST);
    let missing = cfg.missing();
    assert_eq!(
        missing,
        vec!["NXL_TEST_REQUIRED_A"],
        "an unset variable must be reported by name"
    );
}

#[test]
fn set_variables_are_not_missing() {
    // A variable unique to this test, so parallel tests cannot collide.
    std::env::set_var("NXL_TEST_REQUIRED_B", "value-that-must-not-be-printed");
    let cfg = DeployConfig {
        required_env: vec!["NXL_TEST_REQUIRED_B".to_owned()],
    };
    assert!(cfg.missing().is_empty(), "a set variable is present");
    std::env::remove_var("NXL_TEST_REQUIRED_B");
}

#[test]
fn validate_startup_lists_missing_variables() {
    let dir = manifest_dir("startup_missing", MANIFEST);
    let error = cli::validate_startup(&dir).expect_err("missing required env must fail");
    assert_eq!(error.code, "NXL12011");
    assert!(
        error.message.contains("NXL_TEST_REQUIRED_A"),
        "{}",
        error.message
    );
    assert!(
        !error.message.contains("value-that-must-not-be-printed"),
        "diagnostics must never carry variable values"
    );
}

#[test]
fn validate_startup_passes_when_config_and_env_are_sane() {
    // Its own manifest and variable: another test asserts this exact
    // variable is *missing*, so sharing it would race in-process.
    std::env::set_var("NXL_TEST_REQUIRED_OK", "set");
    let dir = manifest_dir(
        "startup_ok",
        "[web]\nhost = \"127.0.0.1\"\nport = 8080\n\n[deploy]\nrequired_env = [\"NXL_TEST_REQUIRED_OK\"]\n",
    );
    let cfg = cli::validate_startup(&dir).expect("valid manifest, var set");
    assert_eq!(cfg.environment, "development");
    std::env::remove_var("NXL_TEST_REQUIRED_OK");
}

// ── graceful shutdown ────────────────────────────────────────────────────────

#[test]
fn serve_stops_when_the_shutdown_signal_completes() {
    let mut app = nuf::App::new();
    app.get("/api/health", |_| async {
        nuf::json_response(&serde_json::json!({"ok": true}))
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // Bind an ephemeral port so parallel tests never collide.
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = runtime.spawn(async move {
        axum::serve(listener, app.into_router())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!server.is_finished(), "the server should be running");

    shutdown_tx.send(()).unwrap();
    runtime
        .block_on(async { tokio::time::timeout(std::time::Duration::from_secs(5), server).await })
        .expect("the server must stop after shutdown")
        .expect("the server task must not panic")
        .expect("graceful shutdown must not error");
    let _ = addr; // bound address is only needed for the sleep sanity check
}

// ── secret handling in database errors ───────────────────────────────────────

#[test]
fn redact_masks_secret_lines() {
    let input = "password=hunter2\nhost=db.example.com\n";
    let out = nxl_compiler::security::redact(input);
    assert!(out.contains("password=***"), "{out}");
    assert!(
        out.contains("db.example.com"),
        "non-secrets stay visible: {out}"
    );
    assert!(
        !out.contains("hunter2"),
        "the secret must not survive: {out}"
    );
}

#[test]
fn a_clean_manifest_produces_no_startup_problems() {
    let dir = manifest_dir("clean", "[web]\nhost = \"127.0.0.1\"\nport = 8080\n");
    let cfg: WebConfig = cli::validate_startup(&dir).expect("clean manifest");
    assert_eq!(cfg.port, 8080);
}

#[test]
fn validate_startup_accepts_a_missing_manifest() {
    // A project with no nxl.toml falls back to defaults and still starts.
    let dir = std::env::temp_dir().join("nxl_deploy_no_manifest");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg: WebConfig = cli::validate_startup(Path::new(&dir)).expect("defaults are valid");
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 8080);
}
