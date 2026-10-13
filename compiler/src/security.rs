//! P16 — Security Engineering
//!
//! Real security primitives for the NEXORA platform. Never invent cryptography:
//! password hashing uses argon2id (vetted), randomness comes from the OS CSPRNG.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | Secure randomness (OS CSPRNG) | IMPLEMENTED |
//! | Password hashing (argon2id) + verify | IMPLEMENTED |
//! | Constant-time comparison | IMPLEMENTED |
//! | Token generation (URL-safe) | IMPLEMENTED |
//! | Output encoding (HTML/JS/URL) | IMPLEMENTED |
//! | Path traversal protection | IMPLEMENTED |
//! | Command injection avoidance (no-shell spawn) | IMPLEMENTED |
//! | SSRF host/IP blocking (private, loopback, link-local) | IMPLEMENTED |
//! | Rate limiting (token bucket) | IMPLEMENTED |
//! | Capability permissions model | IMPLEMENTED |
//! | Secret redaction in logs | IMPLEMENTED |
//! | Secure response headers | IMPLEMENTED |
//! | Threat model document | DESIGNED (docs) |
//! | Fuzzing integration | DEFERRED to P23 |
//! | Dependency scanning | DEFERRED to P28 |

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use rand::RngExt;

// ── Randomness & tokens ───────────────────────────────────────────────

/// Fill `n` bytes with OS-backed secure randomness.
pub fn secure_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::rng().fill(&mut buf[..]);
    buf
}

/// URL-safe token of `n` random bytes, hex-encoded (2n chars).
pub fn generate_token(n: usize) -> String {
    secure_bytes(n).iter().map(|b| format!("{b:02x}")).collect()
}

// ── Password hashing ──────────────────────────────────────────────────

use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};

/// Hash a password with argon2id and a randomly generated salt (PHC format).
pub fn hash_password(password: &str) -> Result<String, String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| format!("hash error: {e}"))
}

/// Verify a password against a PHC hash string.
pub fn verify_password(password: &str, phc: &str) -> bool {
    let Ok(hash) = PasswordHash::new(phc) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

// ── Constant-time comparison ──────────────────────────────────────────

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ── Output encoding ───────────────────────────────────────────────────

pub fn encode_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn encode_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\x3c"),
            '>' => out.push_str("\\x3e"),
            c => out.push(c),
        }
    }
    out
}

pub fn encode_url(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ── Path traversal protection ─────────────────────────────────────────

/// Join `user` onto `root`, rejecting paths that escape the root.
pub fn safe_join(root: &Path, user: &str) -> Option<PathBuf> {
    let joined = root.join(user);
    // Normalize lexically: reject any ParentDir components before joining.
    for c in Path::new(user).components() {
        if matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        ) {
            return None;
        }
    }
    if joined.starts_with(root) {
        Some(joined)
    } else {
        None
    }
}

// ── Command injection avoidance ───────────────────────────────────────

/// Run a program with an argument vector — never through a shell.
pub fn run_safe(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new(program).args(args).output()
}

// ── SSRF protection ───────────────────────────────────────────────────

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4 == Ipv4Addr::new(0, 0, 0, 0)
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Reject URLs whose host is private/loopback/link-local or a raw private IP.
pub fn ssrf_check(url_host: &str) -> Result<(), String> {
    let lower = url_host.trim().to_lowercase();
    if lower.is_empty() {
        return Err("empty host".into());
    }
    let blocked_names = [
        "localhost",
        "localhost.localdomain",
        "metadata.google.internal",
    ];
    if blocked_names.contains(&lower.as_str()) {
        return Err(format!("host '{url_host}' is not allowed"));
    }
    // Extract the host portion: [v6] or [v6]:port, bare IPv6, or host[:port].
    let host_part = if let Some(stripped) = lower.strip_prefix('[') {
        stripped.split(']').next().unwrap_or(stripped)
    } else if lower.parse::<std::net::Ipv6Addr>().is_ok() {
        lower.as_str()
    } else {
        lower.split(':').next().unwrap_or(lower.as_str())
    };
    if let Ok(ip) = host_part.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(format!("host '{url_host}' resolves to a blocked address"));
        }
    }
    Ok(())
}

// ── Rate limiting ─────────────────────────────────────────────────────

pub struct RateLimiter {
    capacity: usize,
    refill_per_sec: f64,
    tokens: Mutex<HashMap<String, (f64, Instant)>>,
}

impl RateLimiter {
    pub fn new(capacity: usize, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Returns true if the request from `key` is allowed.
    pub fn allow(&self, key: &str) -> bool {
        let mut map = self.tokens.lock().unwrap();
        let (tokens, last) = map
            .entry(key.to_owned())
            .or_insert((self.capacity as f64, Instant::now()));
        let now = Instant::now();
        let elapsed = now.duration_since(*last).as_secs_f64();
        *tokens = (*tokens + elapsed * self.refill_per_sec).min(self.capacity as f64);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

// ── Capability permissions ────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Capabilities {
    grants: HashSet<String>,
}

impl Capabilities {
    pub fn new() -> Self {
        Self {
            grants: HashSet::new(),
        }
    }

    pub fn grant(&mut self, cap: &str) {
        self.grants.insert(cap.to_owned());
    }

    pub fn revoke(&mut self, cap: &str) {
        self.grants.remove(cap);
    }

    pub fn check(&self, cap: &str) -> bool {
        self.grants.contains(cap)
    }

    pub fn require(&self, cap: &str) -> Result<(), String> {
        if self.check(cap) {
            Ok(())
        } else {
            Err(format!("missing capability '{cap}'"))
        }
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::new()
    }
}

// ── Secret redaction ──────────────────────────────────────────────────

const SECRET_KEYS: &[&str] = &[
    "password",
    "token",
    "secret",
    "authorization",
    "api_key",
    "apikey",
    "key",
];

/// Redact likely-secret values before logging a header/env dump.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let lower = line.to_lowercase();
        let is_secret = SECRET_KEYS.iter().any(|k| lower.contains(k));
        if is_secret {
            if let Some((k, _)) = line.split_once('=') {
                out.push_str(k.trim());
                out.push_str("=***\n");
            } else if let Some((k, _)) = line.split_once(':') {
                out.push_str(k.trim());
                out.push_str(": ***\n");
            } else {
                out.push_str("***\n");
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

// ── Secure headers ────────────────────────────────────────────────────

/// Headers a production NXL app should set on every response.
pub fn secure_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("content-security-policy", "default-src 'self'"),
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
    ]
}
