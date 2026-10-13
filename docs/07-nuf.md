# NUF — Web Framework (P15)

Backed by axum 0.8 + tokio. Surface:

```rust
let mut app = nuf::App::new();
app.get("/api/health", |_| async { nuf::json_response(&json!({"status":"ok"})) });
app.post("/api/users", |state| async move { /* ... */ });
app.route("/ws/echo", get(nuf::ws_echo_handler));
app.serve(&WebConfig::default())?; // prints to \[host\]:port from [web] in nxl.toml
```

`nxl.toml`:
```toml
[web]
host = "127.0.0.1"
port = 8080
```

Middleware: CORS header + request logger are installed by `App::into_router()`. Additional middleware must use `axum::middleware` on the composed router. Built-in guards: `nuf::auth_guard` (401 unless an authenticated session cookie is present) and `nuf::csrf_guard` (403 for state-changing requests with a session but no matching `x-csrf-token`). Apply them with `from_fn_with_state`:

```rust
let router = app.raw_router()
    .route_layer(axum::middleware::from_fn_with_state(state.clone(), nuf::auth_guard))
    .route_layer(axum::middleware::from_fn_with_state(state.clone(), nuf::csrf_guard))
    .with_state(state.clone());
```

State: `App::with_state(Arc<AppState>)` shares `ServiceContainer`, `SessionStore`, `AuthStore`, `Cache`, `EventBus`, `Queue`.

Sessions: `SessionStore` is in-memory only. `nuf::login(auth, sessions, existing, email, password)` authenticates against `AuthStore` and returns a new session token (destroying any `existing` session to prevent fixation). `nuf::current_user` and `nuf::logout` complete the flow. The built-in `nxl serve` demo does not expose a session route; integrate these into your own handlers and emit the `nxl_session` cookie via `Set-Cookie`.

Authentication: `AuthStore::register(email, password)` hashes with argon2id (via `security::hash_password`), rejecting invalid emails and passwords shorter than 8 characters; `authenticate` verifies in constant time. Authorization is expressed through the `auth_guard` middleware.

CSRF: `nuf::csrf_token(sessions, token)` returns (and lazily creates) the per-session token; `nuf::csrf_valid` compares in constant time. `csrf_guard` enforces the double-submit for POST/PUT/PATCH/DELETE when a session cookie is present.

OpenAPI: routes registered through `App::get`/`App::post` are captured as operations. `App::describe(method, path, summary, protected)` annotates them. `App::openapi(title, version)` returns a 3.0.3 `serde_json::Value` and `App::openapi_json(...)` a string; `nxl serve` exposes it at `/openapi.json`.

WebSockets: `/ws/echo` and the `ws_broadcast_handler` route. NXL-native `route` blocks compiling to these are NOT IMPLEMENTED.

Persistent sessions (database-backed) are NOT IMPLEMENTED — sessions live in memory for the process lifetime.
