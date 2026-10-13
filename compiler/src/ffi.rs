//! P17 — FFI / Interoperability
//!
//! NEXORA uses a *generalized bridge architecture*: each guest language gets an
//! appropriate mechanism rather than one fake universal adapter.
//!
//! # Bridge inventory
//!
//! | Guest | Mechanism | Status |
//! | --- | --- | --- |
//! | Python | subprocess, line-delimited JSON-RPC | IMPLEMENTED |
//! | JavaScript/Node | subprocess, line-delimited JSON-RPC | IMPLEMENTED |
//! | PHP | subprocess, line-delimited JSON-RPC | IMPLEMENTED |
//! | C#/.NET | subprocess JSON-RPC | NOT IMPLEMENTED (dotnet present; harness pending) |
//! | C ABI | libloading dynamic calls | NOT IMPLEMENTED (native backend P07 pending) |
//! | Rust | compiled-in rlib embedding | DEFERRED |
//! | C++ | C ABI shim | BLOCKED (no toolchain) |
//! | Go | cgo / RPC | BLOCKED (no toolchain) |
//! | Java/JVM | JNI / RPC | BLOCKED (no toolchain) |
//! | Dart | FFI / RPC | BLOCKED (no toolchain) |
//! | WASM | component model | BLOCKED (native/wasm backend P07) |
//!
//! Every bridge defines:
//! - type conversion: JSON values cross the boundary (numbers, strings, booleans,
//!   null, arrays, objects); richer types needs generated bindings per language
//! - memory ownership: all payloads are copied; the bridge owns its child process
//! - error propagation: guest exceptions become `{"error": "..."}` frames
//! - threading rules: calls are serialized per bridge instance (single in-flight request)
//! - security boundary: the guest runs as a child process with its own environment;
//!   no in-process execution, so a crashed guest cannot corrupt the host
//! - version compatibility: frames carry `"protocol": "1"`; mismatches are rejected
//! - lifecycle: `spawn` on construction, `close` on drop; one bridge per guest

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

// ── Errors ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FfiError {
    Spawn(String),
    Io(String),
    Protocol(String),
    Guest(String),
    Unavailable(String),
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiError::Spawn(m) => write!(f, "error[NXL17001]: failed to spawn guest: {m}"),
            FfiError::Io(m) => write!(f, "error[NXL17002]: bridge I/O error: {m}"),
            FfiError::Protocol(m) => write!(f, "error[NXL17003]: protocol error: {m}"),
            FfiError::Guest(m) => write!(f, "error[NXL17004]: guest error: {m}"),
            FfiError::Unavailable(m) => write!(f, "error[NXL17005]: NOT IMPLEMENTED: {m}"),
        }
    }
}

impl std::error::Error for FfiError {}

// ── Transport ─────────────────────────────────────────────────────────

const PROTOCOL_VERSION: &str = "1";

/// A synchronous subprocess bridge speaking line-delimited JSON-RPC.
pub struct SubprocessBridge {
    pub language: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl SubprocessBridge {
    /// Spawn the guest with `program` and a harness argument `harness_src`.
    /// `guest_args` are extra guest options; `guest_script` is the path to the
    /// user code the harness should load and dispatch against.
    pub fn spawn(
        language: &str,
        program: &str,
        harness_src: &str,
        guest_script: &str,
    ) -> Result<Self, FfiError> {
        let mut cmd = Command::new(program);
        cmd.arg(harness_src)
            .arg(guest_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if std::env::var("NXL_FFI_DEBUG").is_ok() {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        let mut child = cmd
            .spawn()
            .map_err(|e| FfiError::Spawn(format!("'{program}': {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| FfiError::Io("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .map(BufReader::new)
            .ok_or_else(|| FfiError::Io("no stdout".into()))?;
        Ok(Self {
            language: language.to_owned(),
            child,
            stdin,
            stdout,
        })
    }

    /// Call `function(args...)` in the guest. Returns the JSON result or the
    /// guest's error message.
    pub fn call(&mut self, function: &str, args: &[Value]) -> Result<Value, FfiError> {
        let request = json!({
            "protocol": PROTOCOL_VERSION,
            "function": function,
            "args": args,
        });
        let mut line =
            serde_json::to_string(&request).map_err(|e| FfiError::Protocol(e.to_string()))?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| FfiError::Io(e.to_string()))?;
        self.stdin
            .flush()
            .map_err(|e| FfiError::Io(e.to_string()))?;

        let mut response = String::new();
        let n = self
            .stdout
            .read_line(&mut response)
            .map_err(|e| FfiError::Io(e.to_string()))?;
        if n == 0 {
            return Err(FfiError::Protocol("guest closed the pipe".into()));
        }
        let parsed: Value = serde_json::from_str(response.trim())
            .map_err(|e| FfiError::Protocol(format!("invalid JSON from guest: {e}")))?;
        match parsed.get("protocol").and_then(Value::as_str) {
            Some(v) if v == PROTOCOL_VERSION => {}
            _ => return Err(FfiError::Protocol("protocol version mismatch".into())),
        }
        if let Some(err) = parsed.get("error").and_then(Value::as_str) {
            return Err(FfiError::Guest(err.to_owned()));
        }
        Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn close(&mut self) {
        let _ = self
            .stdin
            .write_all(b"{\"protocol\":\"1\",\"function\":\"__exit__\",\"args\":[]}\n");
        let _ = self.stdin.flush();
        let _ = self.child.kill();
    }
}

impl Drop for SubprocessBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

// ── Guest harnesses (embedded) ────────────────────────────────────────

const PYTHON_HARNESS: &str = r#"
import sys, json, traceback
guest = sys.argv[1]
ns = {}
with open(guest, 'r', encoding='utf-8') as f:
    exec(compile(f.read(), guest, 'exec'), ns)
for line in sys.stdin:
    try:
        req = json.loads(line)
        if req.get('function') == '__exit__':
            break
        fn = ns.get(req['function'])
        if fn is None:
            print(json.dumps({'protocol':'1','error':'no such function: ' + req['function']}))
        else:
            print(json.dumps({'protocol':'1','result': fn(*req.get('args', []))}))
    except Exception as e:
        print(json.dumps({'protocol':'1','error': str(e)}))
    sys.stdout.flush()
"#;

const NODE_HARNESS: &str = r#"
const fs = require('fs');
const vm = require('vm');
const readline = require('readline');
const guestPath = process.argv[2];
const src = fs.readFileSync(guestPath, 'utf8');
vm.runInThisContext(src);
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
    try {
        const req = JSON.parse(line);
        if (req.function === '__exit__') { process.exit(0); }
        const fn = globalThis[req.function];
        if (typeof fn !== 'function') {
            console.log(JSON.stringify({ protocol: '1', error: 'no such function: ' + req.function }));
        } else {
            Promise.resolve(fn(...(req.args || []))).then(
                (result) => console.log(JSON.stringify({ protocol: '1', result })),
                (err) => console.log(JSON.stringify({ protocol: '1', error: String(err) }))
            );
        }
    } catch (err) {
        console.log(JSON.stringify({ protocol: '1', error: String(err) }));
    }
});
"#;

const PHP_HARNESS: &str = r#"<?php
$guest = $argv[1];
$ns = [];
include $guest;
while (($line = fgets(STDIN)) !== false) {
    $req = json_decode($line, true);
    if ($req['function'] === '__exit__') { break; }
    $fn = $req['function'];
    if (!function_exists($fn)) {
        echo json_encode(['protocol' => '1', 'error' => 'no such function: ' . $fn]) . PHP_EOL;
    } else {
        try {
            $result = call_user_func_array($fn, $req['args'] ?? []);
            echo json_encode(['protocol' => '1', 'result' => $result]) . PHP_EOL;
        } catch (Throwable $e) {
            echo json_encode(['protocol' => '1', 'error' => $e->getMessage()]) . PHP_EOL;
        }
    }
    flush();
}
"#;

fn write_harness(language: &str) -> Result<std::path::PathBuf, FfiError> {
    let dir = std::env::temp_dir().join("nxl_ffi");
    std::fs::create_dir_all(&dir).map_err(|e| FfiError::Io(e.to_string()))?;
    let (src, ext) = match language {
        "python" => (PYTHON_HARNESS, "py"),
        "node" => (NODE_HARNESS, "js"),
        "php" => (PHP_HARNESS, "php"),
        other => return Err(FfiError::Unavailable(format!("client for '{other}'"))),
    };
    let path = dir.join(format!("harness.{ext}"));
    std::fs::write(&path, src).map_err(|e| FfiError::Io(e.to_string()))?;
    Ok(path)
}

/// Open a bridge to a guest-language script.
pub fn open(language: &str, guest_script: &str) -> Result<SubprocessBridge, FfiError> {
    let harness = write_harness(language)?;
    let program = match language {
        "python" => "python",
        "node" => "node",
        "php" => "php",
        other => return Err(FfiError::Unavailable(format!("bridge to '{other}'"))),
    };
    SubprocessBridge::spawn(language, program, harness.to_str().unwrap(), guest_script)
}

// ── Capability table (honest status reporting) ────────────────────────

pub fn bridge_table() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("python", "subprocess JSON-RPC (IMPLEMENTED)");
    m.insert("node", "subprocess JSON-RPC (IMPLEMENTED)");
    m.insert("php", "subprocess JSON-RPC (IMPLEMENTED)");
    m.insert("csharp", "subprocess (NOT IMPLEMENTED)");
    m.insert("c", "dynamic loading (NOT IMPLEMENTED)");
    m.insert("rust", "rlib embedding (DEFERRED)");
    m.insert("go", "BLOCKED: no toolchain");
    m.insert("java", "BLOCKED: no toolchain");
    m.insert("wasm", "BLOCKED: native backend pending");
    m
}
