use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use nxl_compiler::nuf;

fn http_get(addr: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).unwrap();
    buf
}

fn http_post(addr: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).unwrap();
    buf
}

fn spawn_test_server() -> (String, Arc<nuf::AppState>) {
    let state = Arc::new(nuf::AppState::default());
    let mut app = nuf::App::with_state(state.clone());
    app.get("/api/health", |_| async {
        nuf::json_response(&serde_json::json!({ "status": "ok" }))
    });
    app.get("/api/users/{id}", |_| async {
        nuf::json_response(&serde_json::json!({ "id": 42 }))
    });
    app.post("/api/users", |_| async {
        nuf::json_response(&serde_json::json!({ "created": true }))
    });
    let config = nuf::WebConfig {
        host: "127.0.0.1".into(),
        port: 0,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::mem::forget(rt);
    let router = app.into_router();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            axum::serve(listener, router).await.unwrap();
        });
    });
    let _ = config;
    (addr, state)
}

#[test]
fn serves_json_health_endpoint() {
    let (addr, _state) = spawn_test_server();
    std::thread::sleep(Duration::from_millis(300));
    let res = http_get(&addr, "/api/health");
    assert!(res.contains("200 OK"), "{res}");
    assert!(res.contains("\"status\":\"ok\""), "{res}");
}

#[test]
fn serves_parameterized_route_and_post() {
    let (addr, _state) = spawn_test_server();
    std::thread::sleep(Duration::from_millis(300));
    let get = http_get(&addr, "/api/users/42");
    assert!(get.contains("\"id\":42"), "{get}");
    let post = http_post(&addr, "/api/users", "{\"name\":\"n\"}");
    assert!(post.contains("\"created\":true"), "{post}");
}

#[test]
fn validation_reports_missing_fields() {
    let errs = nuf::validate(&serde_json::json!({ "name": "" }), &["name", "email"]).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("email")));
    assert!(errs.iter().any(|e| e.contains("empty")));
    assert!(nuf::validate(
        &serde_json::json!({ "name": "a", "email": "b" }),
        &["name", "email"]
    )
    .is_ok());
}

#[test]
fn sessions_create_read_destroy() {
    let store = nuf::SessionStore::new();
    let token = store.create();
    store.set(&token, "user", "alice");
    assert_eq!(store.get(&token, "user").as_deref(), Some("alice"));
    store.destroy(&token);
    assert_eq!(store.get(&token, "user"), None);
}

#[test]
fn cache_expires_entries() {
    let cache = nuf::Cache::new(Duration::from_millis(50));
    cache.set("k", "v");
    assert_eq!(cache.get("k").as_deref(), Some("v"));
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(cache.get("k"), None);
}

#[test]
fn queue_and_event_bus() {
    let q = nuf::Queue::new();
    q.push("job1");
    q.push("job2");
    assert_eq!(q.len(), 2);
    assert_eq!(q.pop().as_deref(), Some("job1"));

    let bus = nuf::EventBus::new();
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits2 = hits.clone();
    bus.on(
        "save",
        Arc::new(move |_| {
            hits2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    );
    bus.emit("save", "payload");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn service_container_registers_and_downcasts() {
    let mut c = nuf::ServiceContainer::new();
    c.register("answer", 42i64);
    assert_eq!(*c.get::<i64>("answer").unwrap(), 42);
    assert!(c.get::<String>("answer").is_none());
}

#[test]
fn web_config_parses_toml() {
    let cfg = nuf::WebConfig::parse("[web]\nhost = \"0.0.0.0\"\nport = 9000\n");
    assert_eq!(cfg.host, "0.0.0.0");
    assert_eq!(cfg.port, 9000);
}

#[test]
fn auth_store_registers_and_authenticates() {
    let auth = nuf::AuthStore::new();
    assert!(auth.register("not-an-email", "password123").is_err());
    assert!(auth.register("a@b.com", "short").is_err());
    let user = auth.register("Alice@Example.com", "password123").unwrap();
    assert_eq!(user.id, 1);
    assert_eq!(user.email, "alice@example.com");
    assert_eq!(auth.user_count(), 1);
    assert!(auth.register("alice@example.com", "password123").is_err());
    assert!(auth.authenticate("alice@example.com", "password123").is_some());
    assert!(auth.authenticate("alice@example.com", "wrong").is_none());
    assert!(auth.authenticate("nobody@example.com", "password123").is_none());
    assert!(auth.find("ALICE@example.com").is_some());
}

#[test]
fn login_creates_session_and_csrf_roundtrip() {
    let auth = nuf::AuthStore::new();
    auth.register("alice@example.com", "password123").unwrap();
    let sessions = nuf::SessionStore::new();

    assert!(nuf::login(&auth, &sessions, None, "alice@example.com", "bad").is_none());

    let token = nuf::login(&auth, &sessions, None, "alice@example.com", "password123").unwrap();
    assert_eq!(
        nuf::current_user(&sessions, &token).as_deref(),
        Some("alice@example.com")
    );

    let csrf = nuf::csrf_token(&sessions, &token);
    assert!(nuf::csrf_valid(&sessions, &token, &csrf));
    assert!(!nuf::csrf_valid(&sessions, &token, "wrong"));
    assert!(!nuf::csrf_valid(&sessions, "no-such-session", &csrf));
    assert_eq!(nuf::csrf_token(&sessions, &token), csrf);

    nuf::logout(&sessions, &token);
    assert!(nuf::current_user(&sessions, &token).is_none());
}

#[test]
fn openapi_generation_lists_operations() {
    let mut app = nuf::App::new();
    app.get("/api/health", |_| async {
        nuf::json_response(&serde_json::json!({ "status": "ok" }))
    });
    app.post("/api/users", |_| async {
        nuf::json_response(&serde_json::json!({}))
    });
    app.describe("get", "/api/health", "Health check", false);
    app.describe("post", "/api/users", "Create a user", true);

    let doc = app.openapi("Demo API", "1.0.0");
    assert_eq!(doc["openapi"].as_str(), Some("3.0.3"));
    assert_eq!(doc["info"]["title"].as_str(), Some("Demo API"));
    assert_eq!(
        doc["paths"]["/api/health"]["get"]["summary"].as_str(),
        Some("Health check")
    );
    assert_eq!(app.operations().len(), 2);
    assert!(doc["paths"]["/api/health"]["get"]["security"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        doc["paths"]["/api/users"]["post"]["security"][0]["session"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

fn http_req(addr: &str, method: &str, path: &str, extra: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).unwrap();
    buf
}

#[test]
fn auth_and_csrf_guards_protect_routes() {
    let state = Arc::new(nuf::AppState::default());
    state
        .auth
        .register("alice@example.com", "password123")
        .unwrap();
    let mut app = nuf::App::with_state(state.clone());
    app.get("/secure", |_| async {
        nuf::json_response(&serde_json::json!({ "ok": true }))
    });
    app.post("/secure", |_| async {
        nuf::json_response(&serde_json::json!({ "saved": true }))
    });

    let router = app
        .raw_router()
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            nuf::auth_guard,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            nuf::csrf_guard,
        ))
        .with_state(state.clone());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::mem::forget(rt);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            axum::serve(listener, router).await.unwrap();
        });
    });
    std::thread::sleep(Duration::from_millis(300));

    let unauth = http_req(&addr, "GET", "/secure", "", "");
    assert!(unauth.contains("401"), "{unauth}");

    let token = nuf::login(
        &state.auth,
        &state.sessions,
        None,
        "alice@example.com",
        "password123",
    )
    .unwrap();
    let cookie = format!("Cookie: nxl_session={token}\r\n");

    let authed = http_req(&addr, "GET", "/secure", &cookie, "");
    assert!(authed.contains("200 OK"), "{authed}");
    assert!(authed.contains("\"ok\":true"), "{authed}");

    let no_csrf = http_req(&addr, "POST", "/secure", &cookie, "{}");
    assert!(no_csrf.contains("403"), "{no_csrf}");

    let csrf = nuf::csrf_token(&state.sessions, &token);
    let both = format!("Cookie: nxl_session={token}\r\nX-CSRF-Token: {csrf}\r\n");
    let ok = http_req(&addr, "POST", "/secure", &both, "{}");
    assert!(ok.contains("200 OK"), "{ok}");
    assert!(ok.contains("\"saved\":true"), "{ok}");
}
