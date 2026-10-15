//! Development server for NXL projects.
//!
//! Compiles the project to JavaScript, serves it with a generated HTML host
//! page, watches the sources and recompiles on change, and pushes a reload
//! signal over Server-Sent Events.
//!
//! What this is and is not:
//!
//! - It is a real HTTP server for the **JavaScript backend**: the program runs
//!   in the browser, so concurrency (`spawn`/`await`/channels) is not
//!   available there — the compiler refuses it, as it does for any JS target.
//! - It is **not** a server-side NXL runtime and not a framework. NXL has no
//!   HTTP primitives in the language, so there is nothing for a server-side
//!   handler to be written in yet. `serve` (NUF) covers the server-side
//!   demonstration, and the NXL language would need web types before a real
//!   server-side story exists.

use crate::diagnostic::Diagnostic;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How the dev server should behave.
#[derive(Debug, Clone)]
pub struct DevConfig {
    /// Address to bind, e.g. `127.0.0.1:3000`.
    pub address: String,
    /// Rebuild automatically when a source file changes.
    pub watch: bool,
    /// Seconds between polls of the source tree.
    pub poll_interval: u64,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:3000".to_owned(),
            watch: true,
            poll_interval: 1,
        }
    }
}

/// A successful compile of the project.
#[derive(Debug, Clone)]
pub struct DevBundle {
    /// The compiled JavaScript program.
    pub script: String,
    /// Diagnostics collected while compiling (warnings or recoverable errors).
    pub diagnostics: Vec<String>,
}

/// Compile a project into a bundle the dev server can serve.
pub fn build(dir: &Path) -> Result<DevBundle, Diagnostic> {
    let config = crate::cli::ProjectConfig::load(dir).map_err(|e| {
        Diagnostic::error(
            "NXL12006",
            format!("{} [{}]", e.message, e.code),
            Default::default(),
        )
    })?;
    let entry = dir.join(&config.entry);
    let module = crate::project::compile_project(&entry)?;
    let artifact = crate::backend::compile_js_module(&module)?;
    Ok(DevBundle {
        script: artifact.source,
        diagnostics: Vec::new(),
    })
}

/// The HTML page that loads the compiled program.
pub fn host_page(project_name: &str, script_path: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{name} — NXL dev server</title>
<style>
  body {{ font: 14px/1.5 system-ui, sans-serif; margin: 0; padding: 1.5rem; background: #11151c; color: #e6e9ef; }}
  h1 {{ font-size: 1.1rem; margin: 0 0 .25rem; }}
  p.sub {{ margin: 0 0 1rem; color: #8b94a7; }}
  #out {{ background: #171c26; border: 1px solid #262d3a; border-radius: 8px; padding: 1rem; white-space: pre-wrap; }}
  #status {{ color: #8b94a7; margin-bottom: 1rem; }}
  .ok {{ color: #7ee787; }}
  .bad {{ color: #ff7b72; }}
</style>
</head>
<body>
<h1>{name}</h1>
<p class="sub">Served by <code>nexora dev</code> from the JavaScript backend. Concurrency is VM-only.</p>
<div id="status">connecting…</div>
<div id="out"></div>
<script src="/reload.js"></script>
<script src="{script}"></script>
</body>
</html>
"#,
        name = project_name,
        script = script_path
    )
}

/// The reload client: reconnects and reloads when the server says so.
pub fn reload_script() -> String {
    r#"// Live reload client for the NXL dev server.
(function () {
  var status = document.getElementById('status');
  function connect() {
    var source = new EventSource('/reload');
    source.onopen = function () { status.textContent = 'connected'; status.className = 'ok'; };
    source.onmessage = function (event) {
      if (event.data === 'reload') { source.close(); window.location.reload(); }
    };
    source.onerror = function () {
      status.textContent = 'disconnected, retrying…';
      status.className = 'bad';
      source.close();
      setTimeout(connect, 1000);
    };
  }
  connect();
})();
"#
    .to_owned()
}

/// Snapshot of source contents, used to detect edits cheaply.
///
/// Contents are hashed rather than compared by timestamp: an edit that lands
/// within the filesystem's timestamp resolution would otherwise go unnoticed,
/// which on a fast machine is easy to hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceStamp {
    files: Vec<(PathBuf, String)>,
}

impl SourceStamp {
    /// An empty snapshot, used before the first capture.
    pub fn empty() -> Self {
        Self { files: Vec::new() }
    }

    /// Record the current state of every `.nxl` file under `dir`.
    pub fn capture(dir: &Path) -> Self {
        let mut files = Vec::new();
        collect_sources(dir, &mut files);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Self { files }
    }

    /// `true` when the tree holds no `.nxl` files at all.
    pub fn saw_nothing(&self) -> bool {
        self.files.is_empty()
    }

    /// `true` when any source changed since this snapshot was taken.
    pub fn changed_since(&self, dir: &Path) -> bool {
        // An empty snapshot means "watching has not started yet"; treat the
        // first capture as unchanged so a fresh server does not reload at once.
        if self.files.is_empty() {
            return false;
        }
        Self::capture(dir) != *self
    }

    /// Number of `.nxl` files being watched.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

fn collect_sources(dir: &Path, files: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Build output and caches change as a result of a rebuild and
            // must not trigger one.
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if matches!(
                name.as_deref(),
                Some("build" | ".nxl-cache" | "target" | ".git")
            ) {
                continue;
            }
            collect_sources(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("nxl") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                files.push((path, text));
            }
        }
    }
}

/// Poll interval used when watching.
pub fn poll_interval(config: &DevConfig) -> Duration {
    Duration::from_secs(config.poll_interval.max(1))
}
