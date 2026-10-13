use nxl_compiler::security;

#[test]
fn password_hash_round_trips_and_rejects_wrong_password() {
    let phc = security::hash_password("correct horse battery staple").unwrap();
    assert!(phc.starts_with("$argon2"), "{phc}");
    assert!(security::verify_password(
        "correct horse battery staple",
        &phc
    ));
    assert!(!security::verify_password("wrong", &phc));
}

#[test]
fn token_generation_is_hex_and_random() {
    let a = security::generate_token(16);
    let b = security::generate_token(16);
    assert_eq!(a.len(), 32);
    assert_ne!(a, b);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn constant_time_eq_compares_bytes() {
    assert!(security::constant_time_eq(b"abc", b"abc"));
    assert!(!security::constant_time_eq(b"abc", b"abd"));
    assert!(!security::constant_time_eq(b"abc", b"ab"));
}

#[test]
fn html_encoding_escapes_markup() {
    assert_eq!(
        security::encode_html("<script>alert(\"x\")</script>"),
        "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
    );
}

#[test]
fn js_and_url_encoding_are_safe() {
    assert_eq!(security::encode_js("\"<\n"), "\\\"\\x3c\\n");
    assert_eq!(security::encode_url("a b&c=d"), "a%20b%26c%3Dd");
}

#[test]
fn safe_join_blocks_traversal() {
    let root = std::path::Path::new("/data");
    assert!(security::safe_join(root, "file.txt").is_some());
    assert!(security::safe_join(root, "../etc/passwd").is_none());
    assert!(security::safe_join(root, "sub/../../secret").is_none());
    assert!(security::safe_join(root, "/etc/passwd").is_none());
}

#[test]
fn ssrf_check_blocks_private_and_loopback() {
    assert!(security::ssrf_check("example.com").is_ok());
    assert!(security::ssrf_check("127.0.0.1").is_err());
    assert!(security::ssrf_check("10.0.0.5").is_err());
    assert!(security::ssrf_check("192.168.1.1").is_err());
    assert!(security::ssrf_check("169.254.169.254").is_err());
    assert!(security::ssrf_check("localhost").is_err());
    assert!(security::ssrf_check("[::1]").is_err());
}

#[test]
fn rate_limiter_enforces_capacity() {
    let limiter = security::RateLimiter::new(2, 0.0);
    assert!(limiter.allow("client"));
    assert!(limiter.allow("client"));
    assert!(!limiter.allow("client"));
    // Different key has its own bucket
    assert!(limiter.allow("other"));
}

#[test]
fn capabilities_grant_require_revoke() {
    let mut caps = security::Capabilities::new();
    assert!(caps.require("fs:read").is_err());
    caps.grant("fs:read");
    assert!(caps.require("fs:read").is_ok());
    caps.revoke("fs:read");
    assert!(caps.require("fs:read").is_err());
}

#[test]
fn redact_hides_secrets() {
    let input = "password=hunter2\nAPI_KEY: abcdef\nroute=/home\nAuthorization: Bearer x";
    let out = security::redact(input);
    assert!(!out.contains("hunter2"));
    assert!(!out.contains("abcdef"));
    assert!(!out.contains("Bearer x"));
    assert!(out.contains("route=/home"));
}

#[test]
fn secure_headers_present() {
    let headers = security::secure_headers();
    let names: Vec<&str> = headers.iter().map(|(k, _)| *k).collect();
    assert!(names.contains(&"content-security-policy"));
    assert!(names.contains(&"x-frame-options"));
    assert!(names.contains(&"strict-transport-security"));
}
