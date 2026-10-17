//! Tests for the backend wiring: `serve` runs a real NUF server, and the
//! `db:*` commands run the real database instead of reporting success.

use nxl_compiler::cli;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── `serve` ───────────────────────────────────────────────────────────────────

/// Run the app `serve` builds on an ephemeral port, hand its address to `body`,
/// then stop it.
///
/// The runtime is owned here rather than leaked onto a detached thread: a
/// server thread that is never joined keeps the test binary alive forever.
fn with_server<T>(dir: &Path, body: impl FnOnce(&str) -> T) -> T {
    let app = cli::nuf_demo_app(dir);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let router = app.into_router();
    let task = runtime.spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    std::thread::sleep(Duration::from_millis(250));
    let result = body(&addr);
    task.abort();
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}

fn http_get(addr: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("should connect to the server");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).unwrap();
    body
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nxl_backend_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("public")).unwrap();
    std::fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nentry = \"src/main.nxl\"\n\n[web]\nhost = \"127.0.0.1\"\nport = 8123\n",
    )
    .unwrap();
    std::fs::write(dir.join("public").join("styles.css"), "body{}").unwrap();
    dir
}

#[test]
fn serve_answers_the_health_route() {
    let dir = scratch("health");
    let response = with_server(&dir, |addr| http_get(addr, "/api/health"));
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.contains("\"status\":\"ok\""), "{response}");
}

#[test]
fn serve_reports_the_real_project_on_disk() {
    let dir = scratch("project");
    let response = with_server(&dir, |addr| http_get(addr, "/api/project"));
    assert!(response.contains("200 OK"), "{response}");
    // The name comes from nxl.toml and the file list from the real directory,
    // so this cannot pass against a canned response.
    assert!(response.contains("\"name\":\"fixture\""), "{response}");
    assert!(response.contains("styles.css"), "{response}");
}

#[test]
fn serve_publishes_an_openapi_document_that_describes_itself() {
    let dir = scratch("openapi");
    let response = with_server(&dir, |addr| http_get(addr, "/openapi.json"));
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.contains("\"openapi\":\"3.0.3\""), "{response}");
    assert!(response.contains("/api/health"), "{response}");
    assert!(response.contains("/api/project"), "{response}");
    assert!(response.contains("/openapi.json"), "{response}");
}

#[test]
fn serve_404s_an_unknown_route() {
    let dir = scratch("missing");
    let response = with_server(&dir, |addr| http_get(addr, "/nope"));
    assert!(response.contains("404"), "{response}");
}

// ── `db:*` ────────────────────────────────────────────────────────────────────

fn db_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nxl_backend_db_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("database").join("migrations")).unwrap();
    std::fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"dbfixture\"\nversion = \"0.1.0\"\n\n[database]\ndriver = \"sqlite\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("database").join("migrations").join("001_users.sql"),
        "-- up\nCREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL);\n\n-- down\nDROP TABLE users;\n",
    )
    .unwrap();
    dir
}

#[test]
fn db_create_reports_the_file_it_made() {
    let dir = db_project("create");
    let message = cli::cmd_db_create(&dir).expect("create should succeed");
    assert!(message.contains("Created database"), "{message}");
    assert!(dir.join("database").join("database.sqlite").is_file());
}

#[test]
fn db_migrate_really_applies_the_migration() {
    let dir = db_project("migrate");
    cli::cmd_db_create(&dir).expect("create should succeed");
    let message = cli::cmd_db_migrate(&dir).expect("migrate should succeed");
    assert!(message.contains("001_users.sql"), "{message}");

    let database = std::fs::read(dir.join("database").join("database.sqlite")).unwrap();
    let text = String::from_utf8_lossy(&database);
    assert!(
        text.contains("CREATE TABLE users"),
        "the schema is not in the file"
    );
    assert!(
        text.contains("nxl_migrations"),
        "no migration bookkeeping table"
    );
}

#[test]
fn db_migrate_is_not_reported_twice() {
    let dir = db_project("twice");
    cli::cmd_db_create(&dir).unwrap();
    cli::cmd_db_migrate(&dir).unwrap();
    let second = cli::cmd_db_migrate(&dir).expect("a second migrate should not fail");
    assert_eq!(second, "nothing to migrate");
}

#[test]
fn db_status_tracks_pending_and_applied() {
    let dir = db_project("status");
    cli::cmd_db_create(&dir).unwrap();
    let before = cli::cmd_db_status(&dir).unwrap();
    assert!(before.contains("pending"), "{before}");
    cli::cmd_db_migrate(&dir).unwrap();
    let after = cli::cmd_db_status(&dir).unwrap();
    assert!(after.contains("applied in batch 1"), "{after}");
}

/// True when a table with this name exists in the project's database.
///
/// Asked of the database itself, not by grepping the file: SQLite does not
/// shrink when a table is dropped, so the original `CREATE TABLE` text can sit
/// in a free page long after the table is gone.
fn table_exists(dir: &Path, table: &str) -> bool {
    use nxl_compiler::ndb::Database;
    let mut db = Database::open(dir).expect("should open the database");
    db.query_rows(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
        &[nxl_compiler::ndb::Value::Text(table.to_owned())],
    )
    .map(|rows| !rows.is_empty())
    .unwrap_or(false)
}

#[test]
fn db_rollback_reverts_the_migration() {
    let dir = db_project("rollback");
    cli::cmd_db_create(&dir).unwrap();
    cli::cmd_db_migrate(&dir).unwrap();
    assert!(
        table_exists(&dir, "users"),
        "migrate did not create the table"
    );

    let message = cli::cmd_db_rollback(&dir).expect("rollback should succeed");
    assert!(message.contains("001_users.sql"), "{message}");
    assert!(
        !table_exists(&dir, "users"),
        "the table survived a rollback"
    );
    assert!(cli::cmd_db_status(&dir).unwrap().contains("pending"));
}

#[test]
fn db_audit_records_what_ran() {
    let dir = db_project("audit");
    cli::cmd_db_create(&dir).unwrap();
    assert_eq!(
        cli::cmd_db_audit(&dir).unwrap(),
        "no migration events recorded"
    );
    cli::cmd_db_migrate(&dir).unwrap();
    let audit = cli::cmd_db_audit(&dir).unwrap();
    assert!(audit.contains("001_users.sql"), "{audit}");
    assert!(audit.contains("ok"), "{audit}");
}

#[test]
fn db_drop_removes_the_database() {
    let dir = db_project("drop");
    cli::cmd_db_create(&dir).unwrap();
    let message = cli::cmd_db_drop(&dir).expect("drop should succeed");
    assert!(message.contains("Dropped database"), "{message}");
    assert!(!dir.join("database").join("database.sqlite").exists());
}

#[test]
fn db_commands_report_real_errors_instead_of_success() {
    // A migration with no `-- up` section must fail loudly. This is the case
    // the old stub answered with "db:migrate ok".
    let dir = db_project("broken");
    std::fs::write(
        dir.join("database")
            .join("migrations")
            .join("002_broken.sql"),
        "SELECT 1;\n",
    )
    .unwrap();
    cli::cmd_db_create(&dir).unwrap();
    let error = cli::cmd_db_migrate(&dir).expect_err("a malformed migration must not succeed");
    assert!(
        error.to_string().contains("NDB14004"),
        "expected a migration error, got: {error}"
    );
}

#[test]
fn db_seed_says_when_there_is_nothing_to_seed() {
    let dir = db_project("seed");
    cli::cmd_db_create(&dir).unwrap();
    assert_eq!(cli::cmd_db_seed(&dir).unwrap(), "nothing to seed");
}
