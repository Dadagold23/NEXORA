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

## C-ABI `extern` calls (Phase 5)

`extern` blocks let a program call C functions in dynamically loaded shared libraries (see `11-ffi.md`). Security posture, stated plainly:

- An `extern` declaration is trusted like linking a native binary. A wrong signature is undefined behaviour, not a catchable error. NXL programs using `extern` carry the same risk as a compiled C program.
- Library names and symbol names must be source string literals; there is no runtime composition and no search-path traversal helpers.
- There is no runtime permission gate in v1: writing `extern` is granting the process's full privileges to the call. `--deny-ffi` is a planned follow-up flag.
- Runtime failures (unloadable library, missing symbol, bad argument) are catchable errors (`NXL11013`–`NXL11017`), never panics.
- Only the VM performs the calls; the JS and Rust backends reject them (NXL2007), so no transpiled artifact silently gains native access.

Deferred: live vulnerability-DB scanning (audit currently local), fuzzing integration (P23), DAST/secret scanning (P28). See also `specifications/security-audit-design.md`.
