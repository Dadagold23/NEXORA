//! Axum wiring for the NXL development server.

use crate::dev::{self, DevBundle, DevConfig, SourceStamp};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Response,
};
use axum::routing::get;
use axum::Router;
use futures_util::StreamExt;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared server state.
#[derive(Clone)]
struct DevState {
    project: Arc<PathBuf>,
    name: String,
    config: Arc<DevConfig>,
    bundle: Arc<Mutex<DevBundle>>,
    error: Arc<Mutex<Option<String>>>,
    stamp: Arc<Mutex<SourceStamp>>,
}

/// Build the router that serves a project.
///
/// Kept separate from `serve` so tests can drive it without binding a port.
pub fn router(project: PathBuf, bundle: DevBundle, config: DevConfig) -> Router {
    // Prefer the package name from the manifest; fall back to the directory
    // name when the manifest cannot be read (the error page shows the problem).
    let name = crate::cli::ProjectConfig::load(&project)
        .map(|config| config.name)
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            project
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "nxl".to_owned());
    let stamp = SourceStamp::capture(&project);
    let state = DevState {
        project: Arc::new(project),
        name,
        config: Arc::new(config),
        bundle: Arc::new(Mutex::new(bundle)),
        error: Arc::new(Mutex::new(None)),
        stamp: Arc::new(Mutex::new(stamp)),
    };

    Router::new()
        .route("/", get(index))
        .route("/app.js", get(script))
        .route("/reload.js", get(reload_script))
        .route("/health", get(health))
        .route("/reload", get(reload_events))
        .with_state(state)
}

/// Recompile if any source changed since the last check.
///
/// Called from every request path (not just the event stream) so that `curl`,
/// a health probe and a browser all see the same, current code. Relying on a
/// connected browser alone would mean the server silently served stale code to
/// anyone else.
fn refresh_if_changed(state: &DevState) {
    if !state.config.watch {
        return;
    }
    let changed = match state.stamp.lock() {
        Ok(stamp) => stamp.changed_since(&state.project),
        Err(_) => false,
    };
    if !changed {
        return;
    }
    match dev::build(&state.project) {
        Ok(fresh) => {
            if let Ok(mut guard) = state.bundle.lock() {
                *guard = fresh;
            }
            if let Ok(mut guard) = state.error.lock() {
                *guard = None;
            }
        }
        Err(diagnostic) => {
            if let Ok(mut guard) = state.error.lock() {
                *guard = Some(diagnostic.message.clone());
            }
        }
    }
    if let Ok(mut guard) = state.stamp.lock() {
        *guard = SourceStamp::capture(&state.project);
    }
}

/// `GET /` — the host page, or the last compile error.
async fn index(State(state): State<DevState>) -> Response {
    refresh_if_changed(&state);
    if let Some(error) = state.error.lock().ok().and_then(|e| e.clone()) {
        return error_page(&error).into_response();
    }
    html(&dev::host_page(&state.name, "/app.js")).into_response()
}

/// `GET /app.js` — the compiled program.
async fn script(State(state): State<DevState>) -> Response {
    refresh_if_changed(&state);
    match state.error.lock() {
        Ok(error) if error.is_some() => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            )],
            "// the project does not currently compile\n",
        )
            .into_response(),
        _ => {
            let bundle = state.bundle.lock().map(|b| b.script.clone()).ok();
            match bundle {
                Some(source) => js(&source).into_response(),
                None => (StatusCode::INTERNAL_SERVER_ERROR, "bundle unavailable").into_response(),
            }
        }
    }
}

/// `GET /reload.js` — the live-reload client.
async fn reload_script() -> Response {
    js(&dev::reload_script()).into_response()
}

/// `GET /health` — what a deployment probe or a test can assert on.
async fn health(State(state): State<DevState>) -> Response {
    refresh_if_changed(&state);
    let broken = state.error.lock().map(|e| e.is_some()).unwrap_or(true);
    let body = if broken {
        r#"{"status":"error"}"#.to_owned()
    } else {
        r#"{"status":"ok"}"#.to_owned()
    };
    let status = if broken {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// `GET /reload` — Server-Sent Events; `reload` triggers a browser reload.
async fn reload_events(
    State(state): State<DevState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let project = state.project.clone();
    let watch = state.config.watch;
    let stamp = state.stamp.clone();
    let bundle = state.bundle.clone();
    let error = state.error.clone();
    let interval = dev::poll_interval(&state.config);

    // `unfold` keeps the poll state in the closure, so no extra crates are
    // needed to build the stream.
    let stream = futures_util::stream::unfold(SourceStamp::empty(), move |mut previous| {
        let project = project.clone();
        let stamp = stamp.clone();
        let bundle = bundle.clone();
        let error = error.clone();
        async move {
            let mut event = None;
            if watch && previous.changed_since(&project) {
                match dev::build(&project) {
                    Ok(fresh) => {
                        if let Ok(mut guard) = bundle.lock() {
                            *guard = fresh;
                        }
                        if let Ok(mut guard) = error.lock() {
                            *guard = None;
                        }
                    }
                    Err(diagnostic) => {
                        if let Ok(mut guard) = error.lock() {
                            *guard = Some(diagnostic.message.clone());
                        }
                    }
                }
                if let Ok(mut guard) = stamp.lock() {
                    *guard = SourceStamp::capture(&project);
                }
                previous = SourceStamp::capture(&project);
                event = Some(Ok(Event::default().event("reload").data("reload")));
            }
            tokio::time::sleep(interval).await;
            Some((event, previous))
        }
    })
    .filter_map(|event| async move { event });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Recompile once before the first request, so `/app.js` is never stale.
pub fn compile_initial(project: &Path) -> Result<DevBundle, crate::diagnostic::Diagnostic> {
    dev::build(project)
}

fn html(body: &str) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body.to_owned(),
    )
}

fn js(body: &str) -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.to_owned(),
    )
}

/// A readable page for a project that does not compile.
fn error_page(message: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>compile error</title>
<style>body{{font:14px/1.5 system-ui,sans-serif;margin:0;padding:1.5rem;background:#11151c;color:#e6e9ef}}
pre{{background:#2d1418;border:1px solid #532029;border-radius:8px;padding:1rem;color:#ff7b72;white-space:pre-wrap}}
h1{{font-size:1.1rem}}</style></head>
<body><h1>The project does not compile</h1><pre>{}</pre>
<p>Fix the error; the dev server reloads automatically.</p></body></html>"#,
        escape_html(message)
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Awaitable wrapper so tests can start the server on an ephemeral port.
pub async fn serve_forever(config: DevConfig) -> Result<(), crate::diagnostic::Diagnostic> {
    let project = std::env::current_dir().map_err(|e| {
        crate::diagnostic::Diagnostic::error("NXL12004", e.to_string(), Default::default())
    })?;
    serve_in(project, config).await
}

/// Serve the project in `project` until interrupted.
pub async fn serve_in(
    project: PathBuf,
    config: DevConfig,
) -> Result<(), crate::diagnostic::Diagnostic> {
    use std::net::SocketAddr;

    let bundle = compile_initial(&project)?;
    let address = config.address.clone();
    let app = router(project, bundle, config);
    let socket: SocketAddr = address.parse().map_err(|_| {
        crate::diagnostic::Diagnostic::error(
            "NXL12002",
            format!("'{address}' is not a valid host:port"),
            Default::default(),
        )
    })?;
    let listener = tokio::net::TcpListener::bind(socket).await.map_err(|e| {
        crate::diagnostic::Diagnostic::error(
            "NXL12004",
            format!("cannot listen on {address}: {e}"),
            Default::default(),
        )
    })?;
    println!("NXL dev server listening on http://{address}");
    println!("  edit a source file and the browser reloads automatically");
    println!("  press Ctrl+C to stop");

    // No graceful-shutdown signal: stopping the process is enough for a
    // development server, and it keeps the dependency set unchanged.
    axum::serve(listener, app).await.map_err(|e| {
        crate::diagnostic::Diagnostic::error("NXL12004", e.to_string(), Default::default())
    })
}

/// How long a client should wait for reload events before reconnecting.
pub fn keep_alive_interval() -> Duration {
    Duration::from_secs(15)
}
