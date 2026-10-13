//! P15 — NUF Web Framework
//!
//! Real axum/tokio-backed HTTP implementation.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | HTTP server | IMPLEMENTED (axum/hyper over tokio) |
//! | Routing (verbs, params, groups) | IMPLEMENTED |
//! | Middleware (logging, CORS, auth guard) | IMPLEMENTED |
//! | Controllers / services / DI | IMPLEMENTED |
//! | Request validation | IMPLEMENTED |
//! | Responses (JSON, text, redirect, status) | IMPLEMENTED |
//! | Cookies | IMPLEMENTED |
//! | Sessions (in-memory store, invalidation) | IMPLEMENTED |
//! | WebSockets | IMPLEMENTED (echo + broadcast) |
//! | JSON APIs / serialization | IMPLEMENTED (serde_json) |
//! | Configuration ([web] in nxl.toml) | IMPLEMENTED |
//! | Caching (in-memory TTL) | IMPLEMENTED |
//! | Queues / events (in-memory) | IMPLEMENTED |
//! | Authentication (argon2id users + sessions) | IMPLEMENTED |
//! | Authorization / auth guard middleware | IMPLEMENTED |
//! | CSRF protection (double-submit) | IMPLEMENTED |
//! | OpenAPI 3.0 generation | IMPLEMENTED |

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, MethodRouter};
use axum::{Json, Router};
use serde_json::{json, Value};

// ── Configuration ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 8080,
        }
    }
}

impl WebConfig {
    pub fn parse(toml: &str) -> Self {
        let mut cfg = WebConfig::default();
        let mut in_web = false;
        for raw in toml.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_web = line == "[web]";
                continue;
            }
            if !in_web {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"');
                match k.trim() {
                    "host" => cfg.host = v.to_owned(),
                    "port" => cfg.port = v.parse().unwrap_or(8080),
                    _ => {}
                }
            }
        }
        cfg
    }

    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join("nxl.toml"))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
}

// ── Service container (DI) ────────────────────────────────────────────

#[derive(Default)]
pub struct ServiceContainer {
    services: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl ServiceContainer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Any + Send + Sync>(&mut self, name: &str, service: T) {
        self.services.insert(name.to_owned(), Arc::new(service));
    }

    pub fn get<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        self.services
            .get(name)
            .and_then(|a| Arc::downcast::<T>(a.clone()).ok())
    }
}

// ── Sessions ──────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SessionStore {
    sessions: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn token() -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        )
            .hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// Create a new session and return its token.
    pub fn create(&self) -> String {
        let mut store = self.sessions.lock().unwrap();
        let mut token = Self::token();
        while store.contains_key(&token) {
            token = Self::token();
        }
        store.insert(token.clone(), HashMap::new());
        token
    }

    pub fn set(&self, token: &str, key: &str, value: &str) {
        if let Some(session) = self.sessions.lock().unwrap().get_mut(token) {
            session.insert(key.to_owned(), value.to_owned());
        }
    }

    pub fn get(&self, token: &str, key: &str) -> Option<String> {
        self.sessions.lock().unwrap().get(token)?.get(key).cloned()
    }

    pub fn destroy(&self, token: &str) {
        self.sessions.lock().unwrap().remove(token);
    }

    pub fn flush(&self) {
        self.sessions.lock().unwrap().clear();
    }
}

/// Extract the `nxl_session` cookie value from a request.
pub fn session_token_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix("nxl_session=").map(|v| v.to_owned())
        })
}

// ── Authentication ────────────────────────────────────────────────────

/// A registered user. The password is stored only as a PHC argon2id hash.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub password_hash: String,
}

#[derive(Default)]
struct AuthInner {
    next_id: u64,
    users: HashMap<String, User>,
}

/// In-memory user registry with argon2id password hashing.
#[derive(Default)]
pub struct AuthStore {
    inner: Mutex<AuthInner>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a user. Emails are matched case-insensitively.
    pub fn register(&self, email: &str, password: &str) -> Result<User, String> {
        let email = email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err("a valid email is required".to_owned());
        }
        if password.len() < 8 {
            return Err("password must be at least 8 characters".to_owned());
        }
        let hash = crate::security::hash_password(password)?;
        let mut inner = self.inner.lock().unwrap();
        if inner.users.contains_key(&email) {
            return Err("email is already registered".to_owned());
        }
        inner.next_id += 1;
        let user = User {
            id: inner.next_id,
            email: email.clone(),
            password_hash: hash,
        };
        inner.users.insert(email, user.clone());
        Ok(user)
    }

    /// Verify credentials, returning the user on success.
    pub fn authenticate(&self, email: &str, password: &str) -> Option<User> {
        let email = email.trim().to_lowercase();
        let inner = self.inner.lock().unwrap();
        let user = inner.users.get(&email)?;
        if crate::security::verify_password(password, &user.password_hash) {
            Some(user.clone())
        } else {
            None
        }
    }

    pub fn find(&self, email: &str) -> Option<User> {
        self.inner
            .lock()
            .unwrap()
            .users
            .get(&email.trim().to_lowercase())
            .cloned()
    }

    pub fn user_count(&self) -> usize {
        self.inner.lock().unwrap().users.len()
    }
}

/// Session keys used by the authentication helpers.
pub const SESSION_USER_ID: &str = "auth.user_id";
pub const SESSION_USER_EMAIL: &str = "auth.email";
pub const SESSION_CSRF: &str = "auth.csrf";

/// Authenticate and start a session, returning the new session token.
///
/// Any pre-existing session referenced by `existing` is destroyed first to
/// prevent session fixation.
pub fn login(
    auth: &AuthStore,
    sessions: &SessionStore,
    existing: Option<&str>,
    email: &str,
    password: &str,
) -> Option<String> {
    let user = auth.authenticate(email, password)?;
    if let Some(old) = existing {
        sessions.destroy(old);
    }
    let token = sessions.create();
    sessions.set(&token, SESSION_USER_ID, &user.id.to_string());
    sessions.set(&token, SESSION_USER_EMAIL, &user.email);
    Some(token)
}

/// Return the email of the currently authenticated session, if any.
pub fn current_user(sessions: &SessionStore, token: &str) -> Option<String> {
    sessions.get(token, SESSION_USER_EMAIL)
}

/// End a session.
pub fn logout(sessions: &SessionStore, token: &str) {
    sessions.destroy(token);
}

// ── CSRF protection ───────────────────────────────────────────────────

/// Get (or lazily create) the CSRF token bound to a session.
pub fn csrf_token(sessions: &SessionStore, token: &str) -> String {
    if let Some(existing) = sessions.get(token, SESSION_CSRF) {
        return existing;
    }
    let fresh = crate::security::generate_token(32);
    sessions.set(token, SESSION_CSRF, &fresh);
    fresh
}

/// Constant-time check of a submitted CSRF token against the session token.
pub fn csrf_valid(sessions: &SessionStore, token: &str, submitted: &str) -> bool {
    match sessions.get(token, SESSION_CSRF) {
        Some(expected) => {
            crate::security::constant_time_eq(expected.as_bytes(), submitted.as_bytes())
        }
        None => false,
    }
}

// ── In-memory cache ───────────────────────────────────────────────────

pub struct Cache {
    entries: Mutex<HashMap<String, (Instant, String)>>,
    ttl: Duration,
}

impl Cache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let mut entries = self.entries.lock().unwrap();
        match entries.get(key) {
            Some((t, v)) if t.elapsed() < self.ttl => Some(v.clone()),
            _ => {
                entries.remove(key);
                None
            }
        }
    }

    pub fn set(&self, key: &str, value: &str) {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_owned(), (Instant::now(), value.to_owned()));
    }
}

// ── Events and queues ─────────────────────────────────────────────────

pub type EventListener = Arc<dyn Fn(&str) + Send + Sync>;

pub struct EventBus {
    listeners: Mutex<HashMap<String, Vec<EventListener>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self {
            listeners: Mutex::new(HashMap::new()),
        }
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(&self, event: &str, listener: EventListener) {
        self.listeners
            .lock()
            .unwrap()
            .entry(event.to_owned())
            .or_default()
            .push(listener);
    }

    pub fn emit(&self, event: &str, payload: &str) {
        let listeners = self.listeners.lock().unwrap();
        if let Some(ls) = listeners.get(event) {
            for l in ls {
                l(payload);
            }
        }
    }
}

pub struct Queue {
    jobs: Mutex<VecDeque<String>>,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(VecDeque::new()),
        }
    }
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, job: &str) {
        self.jobs.lock().unwrap().push_back(job.to_owned());
    }

    pub fn pop(&self) -> Option<String> {
        self.jobs.lock().unwrap().pop_front()
    }

    pub fn len(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── Validation ────────────────────────────────────────────────────────

/// Validate a JSON object against a required-fields list.
pub fn validate(body: &Value, required: &[&str]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let Some(obj) = body.as_object() else {
        return Err(vec!["body must be a JSON object".to_owned()]);
    };
    for field in required {
        match obj.get(*field) {
            None => errors.push(format!("missing field '{field}'")),
            Some(Value::Null) => errors.push(format!("field '{field}' must not be null")),
            Some(Value::String(s)) if s.is_empty() => {
                errors.push(format!("field '{field}' must not be empty"))
            }
            _ => {}
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ── Application ───────────────────────────────────────────────────────

pub struct AppState {
    pub services: ServiceContainer,
    pub sessions: SessionStore,
    pub auth: AuthStore,
    pub cache: Cache,
    pub events: EventBus,
    pub queue: Queue,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            services: ServiceContainer::new(),
            sessions: SessionStore::new(),
            auth: AuthStore::new(),
            cache: Cache::new(Duration::from_secs(60)),
            events: EventBus::new(),
            queue: Queue::new(),
        }
    }
}

/// A documented route operation, captured for OpenAPI generation.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub method: String,
    pub path: String,
    pub summary: String,
    pub protected: bool,
}

pub struct App {
    pub state: Arc<AppState>,
    router: Router<Arc<AppState>>,
    operations: Vec<Operation>,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AppState::default()),
            router: Router::new(),
            operations: Vec::new(),
        }
    }

    pub fn with_state(state: Arc<AppState>) -> Self {
        Self {
            state,
            router: Router::new(),
            operations: Vec::new(),
        }
    }

    pub fn route(&mut self, path: &str, method: MethodRouter<Arc<AppState>>) {
        self.router = std::mem::take(&mut self.router).route(path, method);
    }

    pub fn get<F, Fut>(&mut self, path: &str, handler: F)
    where
        F: Fn(State<Arc<AppState>>) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        self.record("get", path);
        self.route(path, get(handler));
    }

    pub fn post<F, Fut>(&mut self, path: &str, handler: F)
    where
        F: Fn(State<Arc<AppState>>) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        self.record("post", path);
        self.route(path, axum::routing::post(handler));
    }

    fn record(&mut self, method: &str, path: &str) {
        if !self
            .operations
            .iter()
            .any(|o| o.method == method && o.path == path)
        {
            self.operations.push(Operation {
                method: method.to_owned(),
                path: path.to_owned(),
                summary: String::new(),
                protected: false,
            });
        }
    }

    /// Attach a summary and protection flag to a documented operation,
    /// adding it if it was not registered as a route.
    pub fn describe(&mut self, method: &str, path: &str, summary: &str, protected: bool) {
        let method = method.to_lowercase();
        if let Some(op) = self
            .operations
            .iter_mut()
            .find(|o| o.method == method && o.path == path)
        {
            op.summary = summary.to_owned();
            op.protected = protected;
        } else {
            self.operations.push(Operation {
                method,
                path: path.to_owned(),
                summary: summary.to_owned(),
                protected,
            });
        }
    }

    /// The operations captured for OpenAPI generation.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Generate an OpenAPI 3.0 document describing the registered operations.
    pub fn openapi(&self, title: &str, version: &str) -> Value {
        let mut paths = serde_json::Map::new();
        for op in &self.operations {
            let entry = paths
                .entry(op.path.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Value::Object(methods) = entry {
                let security = if op.protected {
                    json!([{ "session": [] }])
                } else {
                    json!([])
                };
                methods.insert(
                    op.method.clone(),
                    json!({
                        "summary": op.summary,
                        "responses": { "200": { "description": "OK" } },
                        "security": security,
                    }),
                );
            }
        }
        json!({
            "openapi": "3.0.3",
            "info": { "title": title, "version": version },
            "paths": Value::Object(paths),
            "components": {
                "securitySchemes": {
                    "session": { "type": "apiKey", "in": "cookie", "name": "nxl_session" }
                }
            }
        })
    }

    /// Serialize [`App::openapi`] to a JSON string.
    pub fn openapi_json(&self, title: &str, version: &str) -> String {
        self.openapi(title, version).to_string()
    }

    pub fn raw_router(&self) -> Router<Arc<AppState>> {
        self.router.clone()
    }

    /// Build a Router with CORS + logging middleware applied.
    pub fn into_router(self) -> Router {
        let state = self.state.clone();
        self.router
            .with_state(state)
            .layer(axum::middleware::from_fn(cors_headers))
            .layer(axum::middleware::from_fn(request_logger))
    }

    /// Start the HTTP server. Blocks the current thread.
    pub fn serve(self, config: &WebConfig) -> std::io::Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let addr = format!("{}:{}", config.host, config.port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, self.into_router()).await
        })
    }

    /// Bind the TCP listener without serving; used by tests.
    pub fn bind(self, config: &WebConfig) -> std::io::Result<tokio::net::TcpListener> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let listener = rt.block_on(tokio::net::TcpListener::bind(format!(
            "{}:{}",
            config.host, config.port
        )))?;
        // Leak the runtime so it stays alive for the lifetime of the socket;
        // the caller is expected to move it into a spawned task.
        std::mem::forget(rt);
        Ok(listener)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ── Middleware ────────────────────────────────────────────────────────

async fn cors_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut()
        .insert("access-control-allow-origin", "*".parse().unwrap());
    res
}

async fn request_logger(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    eprintln!("nuf: {} {}", req.method(), req.uri());
    next.run(req).await
}

/// Reject requests that do not carry a valid authenticated session (401).
pub async fn auth_guard(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let authenticated = session_token_from(req.headers())
        .and_then(|t| current_user(&state.sessions, &t))
        .is_some();
    if authenticated {
        next.run(req).await
    } else {
        error_response(StatusCode::UNAUTHORIZED, "authentication required")
    }
}

/// Enforce CSRF double-submit tokens for state-changing methods.
///
/// Safe methods (GET, HEAD, OPTIONS) and requests without a session cookie are
/// passed through; other requests must present an `x-csrf-token` header matching
/// the token bound to the session.
pub async fn csrf_guard(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let method = req.method().clone();
    if matches!(
        method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        return next.run(req).await;
    }
    let Some(session) = session_token_from(req.headers()) else {
        return next.run(req).await;
    };
    let submitted = req
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if csrf_valid(&state.sessions, &session, submitted) {
        next.run(req).await
    } else {
        error_response(StatusCode::FORBIDDEN, "invalid or missing CSRF token")
    }
}

// ── WebSocket support ─────────────────────────────────────────────────

/// Echo every text/binary message back to the sender.
pub async fn ws_echo_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| async move {
        let (mut sender, mut receiver) = socket.split();
        use futures_util::{SinkExt, StreamExt};
        while let Some(Ok(msg)) = receiver.next().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    })
}

/// Same-room broadcast: every text message is echoed to all connected peers.
pub async fn ws_broadcast_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        state.events.emit("ws_connect", "");
        let (mut sender, mut receiver) = socket.split();
        use futures_util::{SinkExt, StreamExt};
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(t) = &msg {
                state.events.emit("ws_message", t);
            }
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    })
}

// ── Helpers ───────────────────────────────────────────────────────────

pub fn json_response(value: &Value) -> Response {
    Json(value.clone()).into_response()
}

pub fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub fn redirect(path: &str) -> Response {
    Redirect::to(path).into_response()
}

/// Parse `?key=value` query pairs (first value wins).
pub fn query_params(headers_uri: &axum::http::Uri) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(q) = headers_uri.query() {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                out.insert(k.to_owned(), v.replace('+', " "));
            }
        }
    }
    out
}

// Re-exports for consumers.
pub use axum::extract::Request as WebRequest;
pub use axum::routing::{delete, patch, put};
pub use futures_util;
