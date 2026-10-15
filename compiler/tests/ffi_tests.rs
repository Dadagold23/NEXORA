use std::fs;
use std::path::PathBuf;

use nxl_compiler::ffi;

fn tmp_guest(name: &str, ext: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("nxl_ffi_tests");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.{ext}"));
    fs::write(&path, src).unwrap();
    path
}

#[test]
fn python_bridge_calls_functions() {
    let guest = tmp_guest(
        "guest_py",
        "py",
        "def add(a, b):\n    return a + b\n\ndef boom():\n    raise ValueError('kaboom')\n",
    );
    let mut bridge = ffi::open("python", guest.to_str().unwrap()).unwrap();
    let result = bridge
        .call("add", &[serde_json::json!(2), serde_json::json!(40)])
        .unwrap();
    assert_eq!(result, serde_json::json!(42));
    let err = bridge.call("boom", &[]).unwrap_err();
    assert!(err.to_string().contains("kaboom"), "{err}");
    bridge.close();
}

#[test]
fn node_bridge_calls_functions() {
    let guest = tmp_guest(
        "guest_js",
        "js",
        "function shout(s) { return s.toUpperCase() + '!'; }\n",
    );
    let mut bridge = ffi::open("node", guest.to_str().unwrap()).unwrap();
    let result = bridge.call("shout", &[serde_json::json!("hi")]).unwrap();
    assert_eq!(result, serde_json::json!("HI!"));
    bridge.close();
}

#[test]
fn php_bridge_calls_functions() {
    // PHP is not part of the toolchain's requirements, and GitHub's macOS
    // runner does not ship one on PATH. A missing interpreter has to skip, not
    // fail: the bridge is honest about which languages exist, but a test that
    // demands PHP is installed is testing the runner, not the compiler.
    if !interpreter_available("php") {
        return;
    }
    let guest = tmp_guest(
        "guest_php",
        "php",
        "<?php\nfunction double($n) { return $n * 2; }\n",
    );
    let mut bridge = ffi::open("php", guest.to_str().unwrap()).unwrap();
    let result = bridge.call("double", &[serde_json::json!(21)]).unwrap();
    assert_eq!(result, serde_json::json!(42));
    bridge.close();
}

/// Whether an interpreter is on PATH and can report a version.
fn interpreter_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[test]
fn unknown_language_is_honest_error() {
    let err = match ffi::open("haskell", "x.hs") {
        Ok(_) => panic!("expected error for haskell bridge"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("NOT IMPLEMENTED"), "{err}");
}

#[test]
fn guest_error_propagates() {
    let guest = tmp_guest(
        "guest_err",
        "py",
        "def fail():\n    raise RuntimeError('nope')\n",
    );
    let mut bridge = ffi::open("python", guest.to_str().unwrap()).unwrap();
    let err = bridge.call("fail", &[]).unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    bridge.close();
}

#[test]
fn bridge_table_lists_honest_status() {
    let table = ffi::bridge_table();
    assert!(table["python"].contains("IMPLEMENTED"));
    assert!(table["go"].contains("BLOCKED"));
    assert!(table["wasm"].contains("BLOCKED"));
}
