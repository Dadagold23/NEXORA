//! Project-driven web routing: the `[[route]]` table in `nxl.toml`, turned into
//! a real NUF application.
//!
//! A project declares what it serves; the CLI does not invent routes for it:
//!
//! ```toml
//! [web]
//! layout = "layouts/main.nxlhtml"
//! not_found = "404.nxlhtml"
//!
//! [[route]]
//! method = "GET"
//! path = "/about"
//! page = "about.nxlhtml"
//!
//! [[route]]
//! method = "GET"
//! path = "/posts/:id"
//! page = "post.nxlhtml"
//!
//! [[route]]
//! method = "POST"
//! path = "/contact"
//! page = "contact.nxlhtml"
//! validate = ["name", "email"]
//! ```
//!
//! Pages are `.nxlhtml` templates under `pages/`, wrapped in the layout if one
//! is configured. Path parameters, query parameters and submitted form fields
//! are bound as variables before the page runs, so a template refers to a
//! submitted `name` simply as `name`.
//!
//! # Where the limits are
//!
//! NXL has no HTTP types, so a page is a template, not a handler: it cannot
//! choose a status code, set a header, or call another route. Form validation
//! is declarative — a list of required fields — and failure re-renders the page
//! with `error` bound and status 422. Anything beyond that needs the language to
//! grow request and response types first.

use crate::html;
use crate::nuf;
use crate::source::SourceFile;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{Html, Response};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One declared route.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// `"GET"` or `"POST"`, uppercased.
    pub method: String,
    /// The axum path pattern, with `:name` placeholders.
    pub path: String,
    /// Page file, relative to the project's `pages/` directory.
    pub page: String,
    /// Form fields that must be present and non-empty on a POST.
    pub validate: Vec<String>,
}

impl Route {
    /// The placeholder names in the path, in order.
    pub fn params(&self) -> Vec<String> {
        self.path
            .split('/')
            .filter_map(|segment| segment.strip_prefix(':'))
            .map(str::to_owned)
            .collect()
    }

    /// The path in axum's syntax.
    ///
    /// The manifest uses `:name`, which reads better in a TOML file; axum 0.8
    /// wants `{name}`. Translating here keeps both spellings out of the rest of
    /// the code, and a panic from axum over a stray `:` stays impossible.
    pub fn axum_path(&self) -> String {
        self.path
            .split('/')
            .map(|segment| match segment.strip_prefix(':') {
                None => segment.to_owned(),
                Some(name) => format!("{{{name}}}"),
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Everything a project says about how it is served.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Site {
    pub routes: Vec<Route>,
    /// Layout file, relative to `layouts/`.
    pub layout: Option<String>,
    /// Page used when nothing matches, relative to `pages/`.
    pub not_found: Option<String>,
}

/// A problem with the `[[route]]` table, with the line it is on.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for SiteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl Site {
    /// Read the site's routes from `nxl.toml`.
    ///
    /// Returns `None` when the project declares no routes at all, so callers can
    /// tell "no routes" apart from "routes that failed to parse".
    pub fn load(dir: &Path) -> Result<Option<Self>, SiteError> {
        let path = dir.join("nxl.toml");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return Ok(None),
        };
        Self::parse(&text)
    }

    /// Parse the site's routes from the text of a `nxl.toml`.
    pub fn parse(text: &str) -> Result<Option<Self>, SiteError> {
        let mut site = Site::default();
        // None = not in a route; Some(index) = editing that route.
        let mut current: Option<usize> = None;
        let mut section = "";

        for (number, raw) in text.lines().enumerate() {
            let line = number + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with("[[") {
                if trimmed == "[[route]]" {
                    site.routes.push(Route {
                        method: "GET".to_owned(),
                        path: "/".to_owned(),
                        page: String::new(),
                        validate: Vec::new(),
                    });
                    current = Some(site.routes.len() - 1);
                    section = "route";
                    continue;
                }
                return Err(SiteError {
                    line,
                    message: format!("unknown table `{trimmed}`: the only one is `[[route]]`"),
                });
            }
            if trimmed.starts_with('[') {
                current = None;
                section = if trimmed == "[web]" { "web" } else { "" };
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = unquote(value.trim());

            match section {
                "route" => {
                    let Some(index) = current else { continue };
                    let route = &mut site.routes[index];
                    match key {
                        "method" => {
                            let method = value.to_ascii_uppercase();
                            if method != "GET" && method != "POST" {
                                return Err(SiteError {
                                    line,
                                    message: format!(
                                        "route method '{value}' is not supported (use GET or POST)"
                                    ),
                                });
                            }
                            route.method = method;
                        }
                        "path" => {
                            if !value.starts_with('/') {
                                return Err(SiteError {
                                    line,
                                    message: format!("route path '{value}' must start with `/`"),
                                });
                            }
                            route.path = value.to_owned();
                        }
                        "page" => route.page = value.to_owned(),
                        "validate" => route.validate = string_list(value),
                        other => {
                            return Err(SiteError {
                                line,
                                message: format!(
                                    "unknown route key '{other}' (method, path, page, validate)"
                                ),
                            })
                        }
                    }
                }
                "web" => match key {
                    "layout" => site.layout = Some(value.to_owned()),
                    "not_found" => site.not_found = Some(value.to_owned()),
                    _ => {}
                },
                _ => {}
            }
        }

        for (index, route) in site.routes.iter().enumerate() {
            if route.page.is_empty() {
                return Err(SiteError {
                    line: 0,
                    message: format!("route {} ({}) has no `page`", index + 1, route.path),
                });
            }
            if route
                .path
                .split('/')
                .any(|segment| segment.starts_with(':') && segment.len() < 2)
            {
                return Err(SiteError {
                    line: 0,
                    message: format!("route '{}' has an unnamed `:parameter`", route.path),
                });
            }
        }

        if site.routes.is_empty() && site.layout.is_none() && site.not_found.is_none() {
            return Ok(None);
        }
        Ok(Some(site))
    }

    /// Project's `pages/` directory.
    pub fn pages_dir(dir: &Path) -> PathBuf {
        dir.join("pages")
    }

    /// Project's `layouts/` directory.
    pub fn layouts_dir(dir: &Path) -> PathBuf {
        dir.join("layouts")
    }

    /// The fields every route on this path must bind.
    ///
    /// A form is usually declared as two routes on one path: a GET that draws
    /// it and a POST that validates it. Only the POST carries `validate`, but
    /// the GET has to bind the same names, or the page it draws will not
    /// compile — which is the first thing a visitor sees when they follow a
    /// link to the form. So the union across the path is used for both.
    pub fn required_fields(&self, path: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for route in &self.routes {
            if route.path != path {
                continue;
            }
            for field in &route.validate {
                if !out.contains(field) {
                    out.push(field.clone());
                }
            }
        }
        out
    }

    /// Whether a path matches any declared route.
    pub fn matches(&self, method: &str, path: &str) -> bool {
        self.routes
            .iter()
            .any(|route| route.method == method && pattern_matches(&route.path, path))
    }

    /// The declared methods, for the OpenAPI summary.
    pub fn paths(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .routes
            .iter()
            .map(|route| (route.method.to_lowercase(), route.path.clone()))
            .collect();
        out.sort();
        out
    }
}

/// Whether a declared pattern matches a concrete path.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    if pattern_parts.len() != path_parts.len() {
        return false;
    }
    pattern_parts
        .iter()
        .zip(path_parts.iter())
        .all(|(pattern, actual)| pattern.starts_with(':') || pattern == actual)
}

/// Strip surrounding quotes from a manifest value.
fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Read a manifest value that may be a TOML array or a bare comma-separated
/// list.
///
/// `validate = ["name", "email"]` and `validate = name,email` mean the same
/// thing here. Without the bracket handling, the whole array would arrive as
/// one field called `["name", "email"]` and validation would ask for a field
/// that does not exist.
fn string_list(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    inner
        .split(',')
        .map(unquote)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Percent-decode one path or query segment.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read an `application/x-www-form-urlencoded` body into fields.
fn parse_form(body: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        fields.insert(percent_decode(key), percent_decode(value));
    }
    fields
}

/// Values a page can use without any plumbing of its own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Request {
    /// Path parameters, by name.
    pub params: HashMap<String, String>,
    /// Query string fields.
    pub query: HashMap<String, String>,
    /// Submitted form fields.
    pub form: HashMap<String, String>,
    /// `"GET"` or `"POST"`.
    pub method: String,
    /// Set when validation failed; the page shows it.
    pub error: Option<String>,
}

impl Request {
    /// Names a page can use without the request having supplied them.
    ///
    /// NXL has no way to ask whether a name exists, so a page that reads
    /// `q` on an empty query string would fail to compile if the binding were
    /// only made when the field arrived. These are therefore always bound, to
    /// an empty string when absent: `method` and `path` describe the request,
    /// `error`/`has_error` carry validation state, and `q`/`page` are the
    /// conventional names for a search term and a page number.
    const ALWAYS_BOUND: [&'static str; 6] = ["method", "path", "error", "has_error", "q", "page"];

    /// Every binding a page sees, in a stable order so rendering is repeatable.
    pub fn bindings(&self, path: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, value) in &self.params {
            out.push((name.clone(), value.clone()));
        }
        for (name, value) in &self.query {
            if self.params.contains_key(name) {
                continue;
            }
            out.push((name.clone(), value.clone()));
        }
        for (name, value) in &self.form {
            if self.params.contains_key(name) || self.query.contains_key(name) {
                continue;
            }
            out.push((name.clone(), value.clone()));
        }

        for name in Request::ALWAYS_BOUND {
            if out.iter().any(|(existing, _)| existing == name) {
                continue;
            }
            let value = match name {
                "method" => Some(self.method.clone()),
                "path" => Some(path.to_owned()),
                "error" => Some(self.error.clone().unwrap_or_default()),
                "has_error" => Some(self.error.is_some().to_string()),
                _ => None,
            }
            .unwrap_or_default();
            out.push((name.to_owned(), value));
        }

        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// What a rendered page needs to know about where it came from.
#[derive(Debug, Clone)]
pub struct SiteContext {
    pub root: PathBuf,
    pub site: Site,
}

impl SiteContext {
    pub fn new(root: PathBuf, site: Site) -> Self {
        Self { root, site }
    }

    /// The project's name, for the OpenAPI title.
    pub fn site_name(&self) -> String {
        crate::cli::ProjectConfig::load(&self.root)
            .map(|config| config.name.clone())
            .unwrap_or_else(|_| "nxl".to_owned())
    }

    /// Render a page, wrapping it in the layout if one is configured.
    pub fn render_page(&self, page: &str, request: &Request, path: &str) -> Result<String, String> {
        let file = Site::pages_dir(&self.root).join(page);
        let name = file.display().to_string();
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("could not read page '{}': {error}", file.display()))?;
        let bindings = request.bindings(path);
        let body = Self::render_template(&text, &name, &bindings)?;
        match &self.site.layout {
            None => Ok(body),
            Some(layout) => {
                let path = Site::layouts_dir(&self.root).join(layout);
                let name = path.display().to_string();
                let text = std::fs::read_to_string(&path).map_err(|error| {
                    format!("could not read layout '{}': {error}", path.display())
                })?;
                // The rendered page is markup the author wrote, so it goes in
                // raw; only `error` and the request fields inside it were
                // escaped when their page was rendered.
                let mut bindings = bindings;
                bindings.push(("content".to_owned(), body));
                Self::render_template(&text, &name, &bindings)
            }
        }
    }

    fn render_template(
        text: &str,
        name: &str,
        bindings: &[(String, String)],
    ) -> Result<String, String> {
        let template = html::Template::parse(text).map_err(|error| format!("{name}: {error}"))?;
        let borrowed: Vec<(&str, &str)> = bindings
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        template
            .render_with(name, &borrowed)
            .map_err(|error| format!("{name}: {error}"))
    }

    /// The page to show when nothing matched.
    pub fn render_not_found(&self) -> (StatusCode, String) {
        let page = match &self.site.not_found {
            None => (
                StatusCode::NOT_FOUND,
                "<!doctype html><title>404</title><h1>404</h1><p>Not found.</p>".to_owned(),
            ),
            Some(page) => {
                let request = Request {
                    method: "GET".to_owned(),
                    ..Request::default()
                };
                match self.render_page(page, &request, "") {
                    Ok(page) => (StatusCode::NOT_FOUND, page),
                    // A broken 404 page must still produce a 404, not a 500.
                    Err(error) => (
                        StatusCode::NOT_FOUND,
                        format!("<!doctype html><title>404</title><h1>404</h1><pre>{error}</pre>"),
                    ),
                }
            }
        };
        page
    }
}

/// Check a submitted form against the route's `validate` list.
fn validate(route: &Route, form: &HashMap<String, String>) -> Option<String> {
    if route.validate.is_empty() {
        return None;
    }
    let body = serde_json::Map::new();
    let mut object = body;
    for (key, value) in form {
        object.insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    let required: Vec<&str> = route.validate.iter().map(String::as_str).collect();
    match nuf::validate(&serde_json::Value::Object(object), &required) {
        Ok(()) => None,
        Err(missing) => Some(format!("Please provide: {}", missing.join(", "))),
    }
}

/// Build the NUF application for a project that declares routes.
pub fn build_app(context: Arc<SiteContext>) -> nuf::App {
    let mut app = nuf::App::new();

    for route in context.site.routes.clone() {
        let summary = format!("Rendered from pages/{}", route.page);
        // As served, not as written: the manifest says `:id` but the router matches
        // `{id}`, and the document has to match what the router serves.
        app.describe(
            &route.method.to_lowercase(),
            &route.axum_path(),
            &summary,
            false,
        );

        let handler_context = context.clone();
        let handler_route = route.clone();
        let is_post = route.method == "POST";
        // `App::get`/`App::post` take a single-argument closure, which cannot
        // reach the path, the query or a submitted body, so the router is
        // composed here instead.
        // `Bytes` rather than `String`: axum's `String` extractor refuses bodies that
        // are form-encoded, which is exactly what a form POST sends.
        let handler =
            move |State(_): State<Arc<nuf::AppState>>, uri: Uri, body: axum::body::Bytes| {
                let context = handler_context.clone();
                let route = handler_route.clone();
                async move {
                    let params = extract_params(&route.path, uri.path());
                    let query = uri.query().map(parse_form).unwrap_or_default();
                    let mut request = Request {
                        params,
                        query,
                        method: route.method.clone(),
                        ..Request::default()
                    };
                    if is_post {
                        request.form = parse_form(&String::from_utf8_lossy(&body));
                    }
                    // A field the route declares is bound even when the request did
                    // not supply it, so the page can still be drawn with the form
                    // empty. This matters on GET as well: a form page is reached by
                    // following a link long before anything is submitted, and NXL
                    // cannot ask whether a name exists, so an unbound `name` would
                    // turn the GET into a 500.
                    for field in context.site.required_fields(&route.path) {
                        request.form.entry(field).or_default();
                    }

                    let mut status = StatusCode::OK;
                    if is_post {
                        if let Some(message) = validate(&route, &request.form) {
                            request.error = Some(message);
                            status = StatusCode::UNPROCESSABLE_ENTITY;
                        }
                    }

                    match context.render_page(&route.page, &request, uri.path()) {
                        Ok(page) => (status, Html(page)).into_response(),
                        Err(message) => server_error(&message),
                    }
                }
            };

        let method_router = if is_post {
            axum::routing::post(handler)
        } else {
            axum::routing::get(handler)
        };
        app.route(&route.axum_path(), method_router);
    }

    // `/openapi.json` is registered before the document is generated so that it
    // appears in its own description; the handler reads the finished document
    // lazily, when a request arrives.
    let slot = std::sync::Arc::new(std::sync::OnceLock::<String>::new());
    let title = context.site_name();
    app.describe("get", "/openapi.json", "OpenAPI 3.0 description", false);
    {
        let slot = slot.clone();
        app.route(
            "/openapi.json",
            axum::routing::get(move |State(_): State<Arc<nuf::AppState>>| {
                let slot = slot.clone();
                async move {
                    let document = slot.get().cloned().unwrap_or_default();
                    nuf::json_response(&serde_json::from_str(&document).unwrap_or_default())
                }
            }),
        );
    }
    let _ = slot.set(app.openapi_json(&title, env!("CARGO_PKG_VERSION")));

    // Anything unmatched is the project's 404 page, with a real 404 status.
    let not_found_context = context.clone();
    app.set_fallback(axum::routing::any(
        move |State(_): State<Arc<nuf::AppState>>| {
            let context = not_found_context.clone();
            async move {
                let (status, page) = context.render_not_found();
                (status, Html(page)).into_response()
            }
        },
    ));

    app
}

/// Pull the values of a pattern's `:parameters` out of a concrete path.
fn extract_params(pattern: &str, path: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    if pattern_parts.len() != path_parts.len() {
        return out;
    }
    for (pattern, actual) in pattern_parts.iter().zip(path_parts.iter()) {
        if let Some(name) = pattern.strip_prefix(':') {
            out.insert(name.to_owned(), percent_decode(actual));
        }
    }
    out
}

fn server_error(message: &str) -> Response {
    let escaped = html::escape_html(message);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("<!doctype html><title>500</title><h1>500</h1><pre>{escaped}</pre>"),
    )
        .into_response()
}

use axum::response::IntoResponse;

/// Compile a page from source text, for tests that do not touch the filesystem.
pub fn render_source(text: &str, name: &str) -> Result<String, String> {
    let source = SourceFile::new(name, text.to_owned());
    crate::compile_to_nir(&source)
        .map_err(|diagnostic| diagnostic.message)
        .and_then(|module| crate::vm::execute(&module).map_err(|diagnostic| diagnostic.message))
        .map(|result| result.output.join("\n"))
}
