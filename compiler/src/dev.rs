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
///
/// The error for a directory that is not a project says so, and says what to
/// run: "cannot read .../src/main.nxl" reads like the toolchain is broken.
pub fn build(dir: &Path) -> Result<DevBundle, Diagnostic> {
    if !dir.join("nxl.toml").exists() {
        return Err(Diagnostic::error(
            "NXL12006",
            format!(
                "{} is not an NXL project: there is no nxl.toml here. \
                 Run this inside a project, or create one with `nxlc new web <name>`.",
                dir.display()
            ),
            Default::default(),
        ));
    }
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

/// The project's page, rendered on the server, when it writes one in NXL.
///
/// `public/index.nxl` is a program: it prints the page. It is compiled and run
/// on the VM here, so the HTML a browser receives comes from NXL rather than
/// from a file someone had to keep in step with the app by hand.
///
/// Modules resolve against the project's source directory, so the page can
/// `import app.config` exactly as `src/main.nxl` does.
///
/// `None` means the project has no NXL page; the caller then looks for
/// `public/index.html` and finally for its own generated host page.
pub fn render_page(dir: &Path) -> Option<Result<String, String>> {
    // A `.nxlhtml` template wins over `index.nxl`: asking for one is a
    // deliberate choice, and mixing a template with a plain page would leave
    // two competing definitions of the same route.
    let template = dir.join("public").join("index.nxlhtml");
    if template.is_file() {
        let name = template.display().to_string();
        return Some(match std::fs::read_to_string(&template) {
            Ok(text) => match crate::html::render(&text, &name) {
                Ok(page) => Ok(page),
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(format!("could not read index.nxlhtml: {error}")),
        });
    }
    let page = dir.join("public").join("index.nxl");
    if !page.is_file() {
        return None;
    }
    let module_root = crate::cli::ProjectConfig::load(dir)
        .map(|config| dir.join(&config.entry))
        .ok()
        .and_then(|entry| entry.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| dir.join("src"));
    let module = match crate::project::compile_project_with_modules(&page, &module_root) {
        Ok(module) => module,
        Err(diagnostic) => return Some(Err(diagnostic.message)),
    };
    Some(match crate::vm::execute(&module) {
        Ok(result) => Ok(result.output.join("\n")),
        Err(diagnostic) => Err(diagnostic.message),
    })
}

/// Add the dev-server scripts to a project page that does not load them.
///
/// A hand-written `public/index.html` that forgets `<script src="/app.js">`
/// would render a page with no program on it and no clue why, so the missing
/// tag is inserted rather than left to be discovered in the browser. A page
/// that already loads either script is left untouched.
pub fn inject_scripts(html: &str) -> String {
    let mut out = html.to_owned();
    for source in ["/app.js", "/reload.js"] {
        if html.contains(source) {
            continue;
        }
        let tag = format!("<script src=\"{source}\"></script>");
        match out.rfind("</body>") {
            Some(index) => {
                let (head, tail) = out.split_at(index);
                out = format!("{head}  {tag}\n{tail}");
            }
            None => out.push_str(&format!("\n{tag}\n")),
        }
    }
    out
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

#[cfg(test)]
mod devserver_tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nxl_html_dev_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("public")).expect("should create the scratch project");
        dir
    }

    #[test]
    fn a_template_page_is_rendered_by_the_dev_server() {
        let dir = scratch("template");
        fs::write(
            dir.join("public").join("index.nxlhtml"),
            "<h1>Welcome</h1>\n<nxl>\nlet items = [\"one\", \"two\"];\nfor item in items {\n    print(\"<li>\" + escape_html(item) + \"</li>\");\n}\n</nxl>\n<p><nxl expr='\"v\" + \"0.1.1\"' /></p>",
        )
        .expect("should write the template");
        let rendered = render_page(&dir).expect("a page should be found");
        let page = rendered.expect("the template should render");
        // Each printed piece is its own line, exactly as a plain `index.nxl`
        // page already renders. Pinning the whole string documents that.
        assert_eq!(
            page,
            "<h1>Welcome</h1>\n\n<li>one</li>\n<li>two</li>\n\n<p>\nv0.1.1\n</p>"
        );
    }

    #[test]
    fn template_values_cannot_inject_markup_through_the_dev_server() {
        let dir = scratch("injection");
        fs::write(
            dir.join("public").join("index.nxlhtml"),
            "<div><nxl expr='\"<b>bold</b>\"' /></div>",
        )
        .expect("should write the template");
        let page = render_page(&dir)
            .expect("a page should be found")
            .expect("should render");
        assert!(!page.contains("<b>"), "markup leaked through: {page}");
        assert!(page.contains("&lt;b&gt;"), "{page}");
    }

    #[test]
    fn a_broken_template_reports_why() {
        let dir = scratch("broken");
        fs::write(
            dir.join("public").join("index.nxlhtml"),
            "<nxl>let = ;</nxl>",
        )
        .expect("should write the template");
        let message = render_page(&dir)
            .expect("a page should be found")
            .expect_err("a broken template must not render");
        assert!(!message.is_empty());
    }

    #[test]
    fn a_plain_nxl_page_still_works() {
        let dir = scratch("plain");
        fs::write(
            dir.join("public").join("index.nxl"),
            "print(\"<h1>plain</h1>\");",
        )
        .expect("should write the page");
        let page = render_page(&dir)
            .expect("a page should be found")
            .expect("should render");
        assert!(page.contains("<h1>plain</h1>"), "{page}");
    }
}
