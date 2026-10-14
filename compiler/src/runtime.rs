/// P08 — NXR Runtime
///
/// Provides the execution host for compiled NXL programs.
///
/// # Implementation status
///
/// | Subsystem          | Status                  |
/// |--------------------|-------------------------|
/// | JS execution       | IMPLEMENTED (Node.js)   |
/// | Memory model       | IMPLEMENTED (host GC)   |
/// | Error handling     | IMPLEMENTED             |
/// | Logging            | IMPLEMENTED             |
/// | Environment        | IMPLEMENTED             |
/// | Timers             | STUB (not yet wired)    |
/// | Filesystem         | STUB (not yet wired)    |
/// | Networking         | NOT IMPLEMENTED         |
/// | Async scheduler    | NOT IMPLEMENTED         |
/// | Channels           | NOT IMPLEMENTED         |
/// | Synchronization    | NOT IMPLEMENTED         |
/// | FFI                | NOT IMPLEMENTED         |
/// | Serialization      | NOT IMPLEMENTED         |
/// | Processes          | NOT IMPLEMENTED         |
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;

// ── Public types ──────────────────────────────────────────────────────────────

/// Outcome of executing a compiled NXL program.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionResult {
    /// Standard output captured from the program.
    pub stdout: String,
    /// Standard error captured from the program.
    pub stderr: String,
    /// Process exit code (0 = success).
    pub exit_code: i32,
}

impl ExecutionResult {
    /// Returns `true` if the program exited successfully.
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Configuration for a single program execution.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Maximum wall-clock time allowed for execution.
    pub timeout: Duration,
    /// Additional environment variables injected into the child process.
    pub env: HashMap<String, String>,
    /// Working directory for the child process.
    pub working_dir: Option<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            env: HashMap::new(),
            working_dir: None,
        }
    }
}

/// The NXR runtime host.
///
/// Responsible for compiling a NXL source file to a JS artifact and executing
/// it via the system Node.js interpreter.
pub struct Runtime {
    config: RuntimeConfig,
}

impl Runtime {
    /// Create a runtime with default configuration.
    pub fn new() -> Self {
        Self {
            config: RuntimeConfig::default(),
        }
    }

    /// Create a runtime with custom configuration.
    pub fn with_config(config: RuntimeConfig) -> Self {
        Self { config }
    }

    /// Compile and execute a NXL source file.
    ///
    /// Pipeline: Source → parse → HIR → NIR → JS → Node.js subprocess.
    ///
    /// Returns `Err(Diagnostic)` if compilation fails.
    /// Returns `Ok(ExecutionResult)` with the captured output if Node.js is
    /// available; returns a `RuntimeError` diagnostic if Node.js is not found.
    pub fn run(&self, source: &SourceFile) -> Result<ExecutionResult, Diagnostic> {
        let artifact = crate::backend::compile_js(source)?;
        self.run_artifact(&artifact.source, source)
    }

    /// Execute an already-lowered module (possibly multi-file) via Node.js.
    pub fn run_module(&self, module: &crate::nir::Module) -> Result<ExecutionResult, Diagnostic> {
        let artifact = crate::backend::compile_js_module(module)?;
        let pseudo = SourceFile::new(module.name.clone(), "");
        self.run_artifact(&artifact.source, &pseudo)
    }

    fn run_artifact(
        &self,
        js_source: &str,
        source: &SourceFile,
    ) -> Result<ExecutionResult, Diagnostic> {
        self.execute_js(js_source, source)
    }

    /// Execute a pre-compiled JS string via Node.js.
    fn execute_js(
        &self,
        js_source: &str,
        source: &SourceFile,
    ) -> Result<ExecutionResult, Diagnostic> {
        // Write JS to a temp file so Node.js can execute it.
        let tmp_path = std::env::temp_dir().join(format!(
            "nxl_runtime_{}.js",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));

        std::fs::write(&tmp_path, js_source).map_err(|e| {
            Diagnostic::error(
                "NXL8001",
                format!("runtime: could not write temporary JS file: {e}"),
                crate::source::Span::default(),
            )
        })?;

        let mut cmd = Command::new("node");
        cmd.arg(&tmp_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }
        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            Diagnostic::error(
                "NXL8002",
                format!(
                    "runtime: could not start Node.js — is it installed? ({e})\n\
                     hint: install Node.js from https://nodejs.org or add it to PATH"
                ),
                crate::source::Span::default(),
            )
        })?;

        // Enforce timeout by waiting with a deadline.
        let output = wait_with_timeout(child, self.config.timeout).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            Diagnostic::error(
                "NXL8003",
                format!("runtime: execution timed out or failed: {e}"),
                crate::source::Span::default(),
            )
        })?;

        let _ = std::fs::remove_file(&tmp_path);

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr_text = String::from_utf8_lossy(&output.stderr).into_owned();

        // Surface Node.js runtime errors as diagnostics when exit ≠ 0.
        if exit_code != 0 && !stderr_text.is_empty() {
            // Annotate with source name for context but do not fail — the
            // caller receives the full ExecutionResult and can decide.
            let _ = source; // source name available for future source-map lookup
        }

        Ok(ExecutionResult {
            stdout,
            stderr: stderr_text,
            exit_code,
        })
    }

    /// Returns `true` if a Node.js interpreter is available on PATH.
    pub fn node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

// ── Timeout helper ────────────────────────────────────────────────────────────

/// Wait for a child process, enforcing a wall-clock timeout.
///
/// On Unix this uses `wait_timeout`; on Windows we poll with a short sleep.
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::time::Instant;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process finished — collect output.
                return child.wait_with_output().map_err(|e| e.to_string());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err("execution timed out".to_owned());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

// ── Convenience free function ─────────────────────────────────────────────────

/// Compile and run a NXL source file with default runtime configuration.
///
/// This is the implementation of `nxlc run <file>`.
pub fn run(source: &SourceFile) -> Result<ExecutionResult, Diagnostic> {
    Runtime::new().run(source)
}

/// Compile and run an already-lowered module (possibly multi-file).
pub fn run_module(module: &crate::nir::Module) -> Result<ExecutionResult, Diagnostic> {
    Runtime::new().run_module(module)
}
