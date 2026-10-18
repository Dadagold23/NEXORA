//! Integration tests for the `std::http` and `std::networking` builtins.
//!
//! Every socket test runs against a local `TcpListener` bound to
//! 127.0.0.1 on a random port — the suite never touches the network — and
//! every NXL program is executed on the VM, the only target these blocking
//! builtins support.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nxl_compiler::source::SourceFile;
use nxl_compiler::stdlib::{call_builtin, http, networking, NxlValue};
use nxl_compiler::vm;

/// Start a one-shot HTTP server on a random port. Returns the base URL and
/// a handle that must be joined after the client has run; a panic in the
/// handler then fails the test.
fn serve<F>(handler: F) -> (String, JoinHandle<()>)
where
    F: FnOnce(TcpStream) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("server address");
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept connection");
        handler(stream);
    });
    (format!("http://{address}"), handle)
}

/// Read one full request — headers plus the Content-Length body, if any.
fn read_request(stream: &TcpStream) -> String {
    let mut stream = stream.try_clone().expect("clone test socket");
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read request");
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
            let content_length = head
                .split("\r\n")
                .filter_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .next()
                .unwrap_or(0);
            if buf.len() >= pos + 4 + content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// The body of a fully read request.
fn request_body(request: &str) -> String {
    let pos = find(request.as_bytes(), b"\r\n\r\n").expect("header terminator");
    request[pos + 4..].to_owned()
}

/// Header value from a fully read request, matched case-insensitively.
fn request_header(request: &str, name: &str) -> Option<String> {
    let pos = find(request.as_bytes(), b"\r\n\r\n").expect("header terminator");
    request[..pos].split("\r\n").find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name)
            .then(|| value.trim().to_owned())
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Run an NXL program on the VM and return what it printed.
fn run(name: &str, source: &str) -> Vec<String> {
    let file = SourceFile::new(name, source);
    let result = vm::run(&file).expect("program must run");
    result.output
}

// ── std::http ────────────────────────────────────────────────────────────────

#[test]
fn http_get_returns_status_body_and_headers() {
    let (base, server) = serve(|stream| {
        let request = read_request(&stream);
        assert!(
            request.starts_with("GET /hello HTTP/1.1\r\n"),
            "unexpected request line: {request:?}"
        );
        assert!(request_header(&request, "host").is_some());
        assert_eq!(
            request_header(&request, "connection"),
            Some("close".to_owned())
        );
        let response = "HTTP/1.1 200 OK\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            X-Request-Id: abc123\r\n\
            Content-Length: 5\r\n\r\nworld";
        let mut stream = stream;
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });

    let output = run(
        "http_get.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/hello\");\n\
             print(r.status);\n\
             print(r.body);\n\
             print(std.http.header(r, \"content-type\"));\n\
             print(std.http.header(r, \"X-Request-Id\"));\n\
             print(r.headers);\n"
        ),
    );
    assert_eq!(
        output,
        vec![
            "200",
            "world",
            "text/plain; charset=utf-8",
            "abc123",
            "Map { Content-Type: text/plain; charset=utf-8, X-Request-Id: abc123, Content-Length: 5 }",
        ]
    );
    server.join().expect("test server must not panic");
}

#[test]
fn http_get_field_typo_is_a_catchable_runtime_error() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
    });

    let output = run(
        "http_field.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/\");\n\
             try {{ print(r.nope); }} catch e {{ print(e); }}\n"
        ),
    );
    assert_eq!(output, vec!["unknown field 'nope'"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_wrong_argument_type_is_a_catchable_runtime_error() {
    // No server is needed: the type check fires before any connection.
    let output = run(
        "http_type.nxl",
        "import std.http;\n\
         try { print(std.http.get(123)); } catch e { print(e); }\n",
    );
    assert_eq!(
        output,
        vec!["'std::http::get' argument 1 has the wrong type"]
    );
}

#[test]
fn http_post_sends_json_body_and_returns_the_response() {
    let (base, server) = serve(|stream| {
        let request = read_request(&stream);
        assert!(
            request.starts_with("POST /api/v1 HTTP/1.1\r\n"),
            "unexpected request line: {request:?}"
        );
        let content_type =
            request_header(&request, "content-type").expect("POST must carry a content type");
        assert!(
            content_type.starts_with("application/json"),
            "unexpected content type: {content_type}"
        );
        let body = request_body(&request);
        let response = format!(
            "HTTP/1.1 201 Created\r\nX-Echo: received\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = stream;
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });

    let output = run(
        "http_post.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.post(\"{base}/api/v1\", \"{{\\\"a\\\":1}}\");\n\
             print(r.status);\n\
             print(r.body);\n\
             print(std.http.header(r, \"x-echo\"));\n"
        ),
    );
    assert_eq!(output, vec!["201", "{\"a\":1}", "received"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_error_status_is_data_not_a_failure() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\ngone";
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
    });

    let output = run(
        "http_404.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/missing\");\n\
             print(r.status == 404);\n\
             print(r.status);\n\
             print(r.body);\n"
        ),
    );
    assert_eq!(output, vec!["true", "404", "gone"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_chunked_body_is_decoded() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: chunked\r\n\r\n\
            5\r\nhello\r\n6\r\n world\r\n1\r\n!\r\n0\r\n\r\n";
        let mut stream = stream;
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });

    let output = run(
        "http_chunked.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/stream\");\n\
             print(r.status);\n\
             print(r.body);\n"
        ),
    );
    assert_eq!(output, vec!["200", "hello world!"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_body_without_content_length_reads_to_eof() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 200 OK\r\n\r\nno length announced";
        let mut stream = stream;
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        let _ = stream.shutdown(Shutdown::Write);
    });

    let output = run(
        "http_eof.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/plain\");\n\
             print(r.body);\n"
        ),
    );
    assert_eq!(output, vec!["no length announced"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_redirect_is_returned_not_followed() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\nContent-Length: 0\r\n\r\n";
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
    });

    let output = run(
        "http_redirect.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/old\");\n\
             print(r.status);\n\
             print(std.http.header(r, \"location\"));\n"
        ),
    );
    assert_eq!(output, vec!["302", "/elsewhere"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_https_and_missing_header_are_catchable_errors() {
    let https = run(
        "http_https.nxl",
        "import std.http;\n\
         try { print(std.http.get(\"https://example.com/\")); } catch e { print(e); }\n",
    );
    assert_eq!(
        https,
        vec!["https (TLS) is not supported by std::http yet; use http:// or a local server"]
    );

    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let mut stream = stream;
        let _ = stream.write_all(response.as_bytes());
    });
    let missing = run(
        "http_missing_header.nxl",
        &format!(
            "import std.http;\n\
             let r = std.http.get(\"{base}/\");\n\
             try {{ print(std.http.header(r, \"x-absent\")); }} catch e {{ print(e); }}\n"
        ),
    );
    assert_eq!(missing, vec!["response has no header 'x-absent'"]);
    server.join().expect("test server must not panic");
}

#[test]
fn http_connection_failure_is_reported_with_nxl11012() {
    // Bind and immediately drop a listener: nothing listens on the port.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    drop(listener);

    let error = http::request(
        &format!("http://127.0.0.1:{port}/"),
        "GET",
        None,
        Duration::from_secs(2),
    )
    .expect_err("connecting to a dead port must fail");
    assert_eq!(error.code, "NXL11012");
    assert!(
        error.message.contains("cannot connect"),
        "{}",
        error.message
    );
}

#[test]
fn http_slow_server_times_out() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        // Accept the request, then stay silent well past the client's
        // deadline; the write half is left untouched.
        thread::sleep(Duration::from_millis(700));
        drop(stream);
    });

    let error = http::request(
        &format!("{base}/slow"),
        "GET",
        None,
        Duration::from_millis(150),
    )
    .expect_err("a silent server must time out");
    assert_eq!(error.code, "NXL11012");
    assert!(error.message.contains("timed out"), "{}", error.message);
    server.join().expect("test server must not panic");
}

#[test]
fn http_oversized_response_is_capped() {
    let (base, server) = serve(|stream| {
        let _ = read_request(&stream);
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            http::MAX_RESPONSE_BYTES + 1_000_000
        );
        let mut stream = stream;
        let _ = stream.write_all(head.as_bytes());
        // Push past the cap; the client stops reading and closes, so the
        // remaining writes are expected to fail.
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..(http::MAX_RESPONSE_BYTES / chunk.len() + 8) {
            if stream.write_all(&chunk).is_err() {
                break;
            }
        }
    });

    let error = http::request(&format!("{base}/huge"), "GET", None, Duration::from_secs(5))
        .expect_err("an oversized response must be rejected");
    assert_eq!(error.code, "NXL11012");
    assert!(error.message.contains("exceeds"), "{}", error.message);
    server.join().expect("test server must not panic");
}

#[test]
fn http_dispatch_rejects_bad_arguments_before_connecting() {
    // Wrong scalar type: rejected by the shared type check (NXL11011).
    let error = call_builtin("std::http::get", &[NxlValue::Int(1)])
        .expect_err("Int cannot stand in for a URL");
    assert_eq!(error.code, "NXL11011");

    // A Value parameter accepts structured values: a Map passes the type
    // check and fails in the function itself, not at the gate.
    let error = call_builtin(
        "std::http::header",
        &[NxlValue::Map(Vec::new()), NxlValue::Str("x".to_owned())],
    )
    .expect_err("a map without headers is not a response");
    assert_eq!(error.code, "NXL11012");
    assert!(error.message.contains("expected an HTTP response"));

    // Scalar parameters still reject maps (regression guard for the
    // BuiltinType::Value widening).
    let error = call_builtin(
        "std::strings::len",
        &[NxlValue::List(vec![NxlValue::Int(1)])],
    )
    .expect_err("a list is not a string");
    assert_eq!(error.code, "NXL11011");
}

// ── std::networking ──────────────────────────────────────────────────────────

#[test]
fn networking_resolve_returns_iterable_addresses() {
    let output = run(
        "net_resolve.nxl",
        "import std.networking;\n\
         let addrs = std.networking.resolve(\"localhost\");\n\
         let mut count = 0;\n\
         for ip in addrs { count = count + 1; }\n\
         print(count > 0);\n",
    );
    assert_eq!(output, vec!["true"]);

    match networking::resolve("localhost").expect("localhost must resolve") {
        NxlValue::List(addresses) => {
            assert!(!addresses.is_empty(), "localhost resolved to nothing");
            for address in addresses {
                let NxlValue::Str(address) = address else {
                    panic!("resolve must return strings, got {address:?}");
                };
                address
                    .parse::<std::net::IpAddr>()
                    .expect("every entry must be an IP address");
            }
        }
        other => panic!("resolve must return a list, got {other:?}"),
    }
}

#[test]
fn networking_tcp_probe_reports_reachability() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind probe target");
    let port = listener.local_addr().expect("address").port();

    let open = run(
        "net_probe_open.nxl",
        &format!(
            "import std.networking;\n\
             print(std.networking.tcp_probe(\"127.0.0.1\", {port}, 2000));\n"
        ),
    );
    assert_eq!(open, vec!["true"]);

    drop(listener);

    let closed = run(
        "net_probe_closed.nxl",
        &format!(
            "import std.networking;\n\
             print(std.networking.tcp_probe(\"127.0.0.1\", {port}, 500));\n"
        ),
    );
    assert_eq!(closed, vec!["false"]);
}

#[test]
fn networking_tcp_probe_rejects_unusable_arguments() {
    let error = networking::tcp_probe("127.0.0.1", 0, 100).expect_err("port 0 is not probeable");
    assert_eq!(error.code, "NXL11012");
    let error = networking::tcp_probe("127.0.0.1", 70_000, 100).expect_err("port out of range");
    assert_eq!(error.code, "NXL11012");
    let error = networking::tcp_probe("127.0.0.1", 80, 0).expect_err("zero timeout is unusable");
    assert_eq!(error.code, "NXL11012");
}
