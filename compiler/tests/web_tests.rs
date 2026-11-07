//! Tests for project-driven web routing: the `[[route]]` table, path
//! parameters, layout wrapping, form validation and the 404 page.

use nxl_compiler::web::{Site, SiteContext};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ── The manifest ──────────────────────────────────────────────────────────────

#[test]
fn a_project_with_no_routes_reports_none() {
    assert_eq!(Site::parse("[package]\nname = \"plain\"\n").unwrap(), None);
}

#[test]
fn routes_are_read_with_their_pages() {
    let site = Site::parse(
        "[web]\nlayout = \"main.nxlhtml\"\nnot_found = \"404.nxlhtml\"\n\n[[route]]\nmethod = \"GET\"\npath = \"/\"\npage = \"home.nxlhtml\"\n\n[[route]]\nmethod = \"POST\"\npath = \"/contact\"\npage = \"contact.nxlhtml\"\nvalidate = [\"name\", \"email\"]\n",
    )
    .unwrap()
    .unwrap();
    assert_eq!(site.layout.as_deref(), Some("main.nxlhtml"));
    assert_eq!(site.not_found.as_deref(), Some("404.nxlhtml"));
    assert_eq!(site.routes.len(), 2);
    assert_eq!(site.routes[0].path, "/");
    assert_eq!(site.routes[0].method, "GET");
    assert_eq!(site.routes[1].method, "POST");
    // TOML array syntax has to survive parsing, or validation ends up asking
    // for one field literally named `["name", "email"]`.
    assert_eq!(site.routes[1].validate, vec!["name", "email"]);
}

#[test]
fn a_bare_comma_list_also_works_for_validate() {
    let site =
        Site::parse("[[route]]\npath = \"/x\"\npage = \"x.nxlhtml\"\nvalidate = name, email\n")
            .unwrap()
            .unwrap();
    assert_eq!(site.routes[0].validate, vec!["name", "email"]);
}

#[test]
fn a_route_without_a_page_is_rejected() {
    let error = Site::parse("[[route]]\npath = \"/x\"\n").unwrap_err();
    assert!(error.message.contains("no `page`"), "{}", error.message);
}

#[test]
fn an_unsupported_method_is_rejected() {
    let error =
        Site::parse("[[route]]\nmethod = \"DELETE\"\npath = \"/x\"\npage = \"x.nxlhtml\"\n")
            .unwrap_err();
    assert!(error.message.contains("GET or POST"), "{}", error.message);
}

#[test]
fn a_relative_path_is_rejected() {
    let error = Site::parse("[[route]]\npath = \"about\"\npage = \"a.nxlhtml\"\n").unwrap_err();
    assert!(
        error.message.contains("must start with"),
        "{}",
        error.message
    );
}

#[test]
fn an_unknown_key_is_rejected_rather_than_ignored() {
    let error = Site::parse("[[route]]\npath = \"/x\"\npage = \"a.nxlhtml\"\npge = \"typo\"\n")
        .unwrap_err();
    assert!(
        error.message.contains("unknown route key"),
        "{}",
        error.message
    );
}

#[test]
fn path_parameters_are_listed_and_translated() {
    let site =
        Site::parse("[[route]]\npath = \"/posts/:id/replies/:reply\"\npage = \"p.nxlhtml\"\n")
            .unwrap()
            .unwrap();
    assert_eq!(site.routes[0].params(), vec!["id", "reply"]);
    // axum 0.8 wants `{name}`, the manifest reads better with `:name`.
    assert_eq!(site.routes[0].axum_path(), "/posts/{id}/replies/{reply}");
}

#[test]
fn matching_distinguishes_paths_and_parameters() {
    let site = Site::parse(
        "[[route]]\npath = \"/\"\npage = \"a.nxlhtml\"\n\n[[route]]\npath = \"/posts/:id\"\npage = \"b.nxlhtml\"\n\n[[route]]\nmethod = \"POST\"\npath = \"/posts\"\npage = \"c.nxlhtml\"\n",
    )
    .unwrap()
    .unwrap();
    assert!(site.matches("GET", "/"));
    assert!(site.matches("GET", "/posts/anything"));
    assert!(site.matches("POST", "/posts"));
    assert!(!site.matches("GET", "/posts"));
    assert!(!site.matches("GET", "/nothing"));
}

// ── Serving ───────────────────────────────────────────────────────────────────

/// A minimal project on disk: a layout, two pages and a 404.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nxl_webtest_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("layouts")).unwrap();
    std::fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"fixture\"\n\n[web]\nlayout = \"main.nxlhtml\"\nnot_found = \"404.nxlhtml\"\n\n[[route]]\npath = \"/\"\npage = \"home.nxlhtml\"\n\n[[route]]\npath = \"/posts/:id\"\npage = \"post.nxlhtml\"\n\n[[route]]\npath = \"/contact\"\npage = \"contact.nxlhtml\"\n\n[[route]]\nmethod = \"POST\"\npath = \"/contact\"\npage = \"contact.nxlhtml\"\nvalidate = [\"name\", \"email\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("layouts").join("main.nxlhtml"),
        "<html><nav>NAV</nav><main><nxl raw=\"content\" /></main></html>",
    )
    .unwrap();
    std::fs::write(dir.join("pages").join("home.nxlhtml"), "<h1>Home</h1>").unwrap();
    std::fs::write(
        dir.join("pages").join("post.nxlhtml"),
        "<h1>Post <nxl expr=\"id\" /></h1>",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages").join("contact.nxlhtml"),
        "<nxl>\nif has_error == \"true\" {\n    print(\"<p class=\\\"alert\\\">\");\n    print(escape_html(error));\n    print(\"</p>\");\n}\n</nxl><input value=\"<nxl expr='name' />\">",
    )
    .unwrap();
    std::fs::write(dir.join("pages").join("404.nxlhtml"), "<h1>Missing</h1>").unwrap();
    dir
}

fn request(dir: &Path, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (headers, body) = response(dir, method, target, body);
    let status = headers
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, body)
}

/// The raw head and body of one request against a fresh server for `dir`.
///
/// The head is returned whole so tests can assert on response headers, which
/// the status/body shape would otherwise throw away.
fn response(dir: &Path, method: &str, target: &str, body: Option<&str>) -> (String, String) {
    let app = nxl_compiler::web::build_app(Arc::new(SiteContext::new(
        dir.to_path_buf(),
        Site::load(dir).unwrap().unwrap(),
    )));
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

    let mut stream = TcpStream::connect(&addr).expect("connect");
    let payload = body.unwrap_or("");
    let head = format!(
        "{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(payload.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();

    task.abort();
    runtime.shutdown_timeout(Duration::from_millis(100));

    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    (head.to_owned(), body.to_owned())
}

#[test]
fn a_page_is_rendered_inside_the_layout() {
    let dir = project("layout");
    let (status, body) = request(&dir, "GET", "/", None);
    assert_eq!(status, 200);
    assert!(body.contains("<nav>NAV</nav>"), "{body}");
    assert!(body.contains("<h1>Home</h1>"), "{body}");
}

#[test]
fn a_path_parameter_reaches_the_page() {
    let dir = project("params");
    let (status, body) = request(&dir, "GET", "/posts/42", None);
    assert_eq!(status, 200);
    // The value lands inside the heading, on the same line: a template prints
    // what the author wrote, with the value put where it belongs.
    assert!(body.contains("<h1>Post 42</h1>"), "{body}");
}

#[test]
fn an_unknown_path_returns_the_404_page_with_a_404_status() {
    let dir = project("notfound");
    let (status, body) = request(&dir, "GET", "/nothing/here", None);
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("Missing"), "{body}");
}

// ── Static files ──────────────────────────────────────────────────────────────

fn with_public(name: &str) -> PathBuf {
    let dir = project(name);
    std::fs::create_dir_all(dir.join("public")).unwrap();
    dir
}

#[test]
fn static_assets_are_served_from_public() {
    // The starter's layout links `/styles.css` and `/favicon.svg`; if the site
    // server does not serve `public/`, the page arrives unstyled.
    let dir = with_public("static");
    std::fs::write(
        dir.join("public").join("styles.css"),
        "body { color: red; }",
    )
    .unwrap();
    std::fs::write(
        dir.join("public").join("favicon.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    )
    .unwrap();

    let (head, body) = response(dir.as_path(), "GET", "/styles.css", None);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("content-type: text/css"), "{head}");
    // The safe defaults leave through the same middleware as pages.
    assert!(head.contains("x-content-type-options: nosniff"), "{head}");
    assert!(body.contains("color: red"), "{body}");

    let (status, _) = request(&dir, "GET", "/favicon.svg", None);
    assert_eq!(status, 200);
}

#[test]
fn a_declared_route_wins_over_a_file_of_the_same_name() {
    let dir = with_public("static_precedence");
    std::fs::write(
        dir.join("public").join("index.html"),
        "<h1>static index</h1>",
    )
    .unwrap();
    let (status, body) = request(&dir, "GET", "/", None);
    assert_eq!(status, 200);
    assert!(body.contains("<h1>Home</h1>"), "route must win: {body}");
    assert!(!body.contains("static index"), "{body}");
}

#[test]
fn a_missing_asset_is_the_404_page_not_a_crash() {
    let dir = with_public("static_missing");
    let (status, body) = request(&dir, "GET", "/missing.css", None);
    assert_eq!(status, 404);
    assert!(body.contains("Missing"), "{body}");
}

#[test]
fn compiler_source_is_not_served_by_the_site() {
    // `public/index.nxl` is a program and `.nxlhtml` is a template. Serving
    // their text would show a page's source instead of the page.
    let dir = with_public("static_source");
    std::fs::write(
        dir.join("public").join("page.nxl"),
        "fn secret() { print(\"TOKEN\"); }\n",
    )
    .unwrap();
    std::fs::write(dir.join("public").join("partial.nxlhtml"), "<p>TOKEN</p>").unwrap();

    for path in ["/page.nxl", "/partial.nxlhtml"] {
        let (status, body) = request(&dir, "GET", path, None);
        assert_eq!(status, 404, "{path} must not be served: {body}");
        assert!(!body.contains("TOKEN"), "{path} leaked source: {body}");
    }
}

#[test]
fn public_files_cannot_escape_their_directory() {
    let dir = with_public("static_escape");
    std::fs::write(dir.join("secret.txt"), "do not serve me").unwrap();
    for path in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
        "/sub/../../secret.txt",
    ] {
        let (status, body) = request(&dir, "GET", path, None);
        assert!(
            status == 404 || !body.contains("do not serve me"),
            "{path} must not serve a file outside public/ (status {status})"
        );
    }
}

#[test]
fn a_static_file_is_only_served_on_get() {
    // A file URL is not an endpoint; POST keeps the unrouted-path behavior.
    let dir = with_public("static_post");
    std::fs::write(
        dir.join("public").join("styles.css"),
        "body { color: red; }",
    )
    .unwrap();
    let (status, body) = request(&dir, "POST", "/styles.css", Some(""));
    assert_eq!(status, 404, "{body}");
    assert!(!body.contains("color: red"), "{body}");
}

#[test]
fn a_complete_submission_is_accepted() {
    let dir = project("post_ok");
    let (status, body) = request(
        &dir,
        "POST",
        "/contact",
        Some("name=Ada&email=ada@example.com"),
    );
    assert_eq!(status, 200, "{body}");
    assert!(!body.contains("class=\"alert\""), "{body}");
}

#[test]
fn an_incomplete_submission_is_422_with_a_message() {
    let dir = project("post_bad");
    let (status, body) = request(&dir, "POST", "/contact", Some("name=Ada"));
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("class=\"alert\""), "{body}");
    assert!(body.contains("email"), "{body}");
}

#[test]
fn a_submitted_value_is_escaped() {
    let dir = project("post_xss");
    let (status, body) = request(
        &dir,
        "POST",
        "/contact",
        Some("name=%3Cscript%3Ealert(1)%3C%2Fscript%3E&email=a%40b.c"),
    );
    assert_eq!(status, 200, "{body}");
    assert!(!body.contains("<script>"), "a tag survived: {body}");
    assert!(body.contains("&lt;script&gt;"), "{body}");
}

#[test]
fn a_path_parameter_is_escaped() {
    let dir = project("param_xss");
    let (status, body) = request(&dir, "GET", "/posts/%3Cscript%3E", None);
    assert_eq!(status, 200, "{body}");
    assert!(!body.contains("<script>"), "a tag survived: {body}");
    assert!(body.contains("&lt;script&gt;"), "{body}");
}

#[test]
fn a_post_to_an_unrouted_path_is_404_not_405() {
    let dir = project("post_404");
    let (status, _) = request(&dir, "POST", "/nowhere", Some("x=1"));
    assert_eq!(status, 404);
}

#[test]
fn a_form_page_works_on_get_before_anything_is_submitted() {
    // The GET route on /contact carries no `validate` of its own; only the POST
    // does. The GET still has to bind those names, or following the nav link to
    // the form returns 500. This caught that in review.
    let dir = project("form_get");
    let (status, body) = request(&dir, "GET", "/contact", None);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("<input"), "{body}");
}

#[test]
fn required_fields_are_shared_across_a_path() {
    let site = Site::parse(
        "[[route]]\nmethod = \"GET\"\npath = \"/contact\"\npage = \"c.nxlhtml\"\n\n[[route]]\nmethod = \"POST\"\npath = \"/contact\"\npage = \"c.nxlhtml\"\nvalidate = [\"name\", \"email\"]\n\n[[route]]\npath = \"/other\"\npage = \"o.nxlhtml\"\nvalidate = [\"x\"]\n",
    )
    .unwrap()
    .unwrap();
    assert_eq!(site.required_fields("/contact"), vec!["name", "email"]);
    assert_eq!(site.required_fields("/other"), vec!["x"]);
    assert!(site.required_fields("/missing").is_empty());
}

#[test]
fn a_project_that_declares_routes_still_publishes_openapi() {
    // `docs/03-cli-reference.md` says `serve` exposes `/openapi.json`, so this
    // has to hold for a project with its own routes and not only for the
    // demonstration app.
    let dir = project("openapi");
    let (status, body) = request(&dir, "GET", "/openapi.json", None);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"openapi\":\"3.0.3\""), "{body}");
    assert!(
        body.contains("/posts/{id}"),
        "the project's own routes are missing: {body}"
    );
    assert!(body.contains("/openapi.json"), "{body}");
}

#[test]
fn a_page_renders_the_request_from_the_top_of_the_http_response() {
    // The 404 page of the web starter shows the path it could not match, so the
    // unmatched path has to be bound for the 404 as well as for ordinary pages.
    let dir = project("notfound_path");
    std::fs::write(
        dir.join("pages").join("404.nxlhtml"),
        "<h1>Missing</h1><p><nxl expr=\"path\" /></p>",
    )
    .unwrap();
    let (status, body) = request(&dir, "GET", "/nothing/here", None);
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("/nothing/here"), "{body}");
}

#[test]
fn an_unmatched_path_renders_escaped_in_the_404_page() {
    // The 404 page binds `path`; a crafted path must not become markup there.
    let dir = project("notfound_xss");
    std::fs::write(
        dir.join("pages").join("404.nxlhtml"),
        "<h1>Missing</h1><p><nxl expr=\"path\" /></p>",
    )
    .unwrap();
    let (status, body) = request(&dir, "GET", "/%3Cscript%3Ealert(1)%3C%2Fscript%3E", None);
    assert_eq!(status, 404, "{body}");
    assert!(!body.contains("<script>"), "a tag survived: {body}");
}

#[test]
fn every_response_carries_the_safe_header_defaults() {
    // A page, a 404 and the JSON description all leave through the same
    // middleware, so all three have to carry the headers.
    let dir = project("headers");
    let headers = response(dir.as_path(), "GET", "/", None).0;
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        ("x-frame-options", "DENY"),
    ] {
        assert!(
            headers.contains(&format!("{name}: {value}")),
            "missing '{name}: {value}' in:\n{headers}"
        );
    }

    let not_found = response(dir.as_path(), "GET", "/nowhere", None).0;
    assert!(
        not_found.contains("x-content-type-options: nosniff"),
        "{not_found}"
    );

    let openapi = response(dir.as_path(), "GET", "/openapi.json", None).0;
    assert!(
        openapi.contains("x-content-type-options: nosniff"),
        "{openapi}"
    );
}

#[test]
fn a_layout_can_print_the_project_name_from_the_manifest() {
    // The starter's layout uses `<nxl expr="app_name" />` for its title and
    // brand; the binding comes from the manifest, not the request.
    let dir = project("app_name");
    std::fs::write(
        dir.join("layouts").join("main.nxlhtml"),
        "<html><title><nxl expr=\"app_name\" /></title><main><nxl raw=\"content\" /></main></html>",
    )
    .unwrap();
    let (status, body) = request(&dir, "GET", "/", None);
    assert_eq!(status, 200, "{body}");
    // The fixture manifests `name = "fixture"`, not its directory name.
    assert!(body.contains("<title>fixture</title>"), "{body}");
}
