# Security

Primitives in `compiler/src/security.rs` (all exercised by `compiler/tests/security_tests.rs`):

- hashing: argon2id (`hash_password`/`verify_password`), constant-time `constant_time_eq`
- tokens: `secure_bytes`, hex `generate_token`
- encoding: `encode_html`, `encode_js`, `encode_url`
- path traversal: `safe_join` (rejects `..`, absolute escapes)
- process spawning: `run_safe(program, args)` — no shell ever
- SSRF: `ssrf_check` blocks loopback/private/link-local/reserved, `localhost`, metadata hostnames, bracketed IPv6 literals
- rate limiting: `RateLimiter` token bucket with refill rate
- capabilities: `Capabilities` grant/revoke/require
- log hygiene: `redact()` strips password/token/key/authorization values
- headers: `secure_headers()` produces CSP/HSTS/X-Frame-Options/...

Never invent crypto: only vetted crates (argon2, password-hash) are used.

Deferred: live vulnerability-DB scanning (audit currently local), fuzzing integration (P23), DAST/secret scanning (P28). See also `specifications/security-audit-design.md`.
