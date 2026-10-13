# NEXORA Security Audit Report (P28)

Generated during P28 — Security Audit. Honest status:

## Automated checks available in-tree

| Check | Status | Evidence |
| --- | --- | --- |
| Hard-coded secret scan | PASS | grep for `api_key=`/`password=`/`-----BEGIN` in compiler/src (no matches) |
| Command spawning uses argv (no shell) | PASS | `ffi.rs::SubprocessBridge::spawn`, `security.rs::run_safe`, `stdlib::process::run`, `runtime.rs` — all pass `&[&str]` args; no `cmd /c` or `sh -c` |
| Fuzzing | PASS | 8 fuzz harnesses, 360 tests green |
| Type/semantic safety | PARTIAL | NIR verifier exists in beta; `nxlc check` catches aribable errors; full type system not enforced |
| NDB parameterisation | PASS | Query builder always parameterised; injection test in `ndb_tests` |
| NUF headers | PARTIAL | CORS + request logger middleware only; `secure_headers()` helper exists in security.rs but is not yet wired into `App::into_router` |

## Dependency review

| Crate | Version | Purpose | Known issues |
| --- | --- | --- | --- |
| rusqlite | 0.40.2 | bundled SQLite driver | none known locally |
| axum | 0.8.9 | HTTP framework | none known locally |
| tokio | 1.53 | async runtime | none known locally |
| argon2 | 0.6.0 | password hashing | none known locally |
| serde{,_json} | 1.x | JSON interop | none known locally |
| futures-util | 0.3.34 | ws stream ops | none known locally |
| rand | 0.10.3 | OS CSPRNG wrapper | none known locally |

Live CVE scan is NOT IMPLEMENTED locally (`cargo audit` not installed in the developer environment). A `dependency-audit` job has been added to `.github/workflows/ci.yml` that installs `cargo-audit` and runs it — when CI executes, its result will be reported here.

## Architecture security invariants

- NEXORA compiler is the only place NXL semantics are enforced (P22 invariant), so all future policy checks attach there once
- NDB migrations run inside per-migration transactions; failing seeders never persist partial state
- `nxl db:fresh` is intentionally destructive — guarded by project cwd and only touches `[database].path`, never arbitrary files
- FFI guests run out-of-process with piped stdio and `__exit__` protocol; host never exec'd in-process guest code
- `nxl serve` binds to `127.0.0.1` by default

## Risks & TODO (honest)

- No live CVE DB scan (cargo-audit missing) — enable in CI
- No DAST against `nxl serve`
- No secret scanning in release pipeline
- No SBOM generation in release pipeline
- NUF `secure_headers()` not yet wired to all responses (wire in next hardening pass)
