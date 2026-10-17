# Testing, Fuzzing, Performance, CI

## Pyramid

- Unit/integration: `compiler/tests/*_tests.rs` (355+ passing)
- Compiler acceptance: `compiler/tests/compiler_tests.rs` (266)
- Frontend regression: `language_frontend_tests.rs` (13)
- Subsystem suites: `ndb_tests` (11), `nuf_tests` (8), `nui_tests` (7), `nls_tests` (8), `ffi_tests` (6), `security_tests` (11), `platforms_tests` (5), `debugger_tests` (6), `ide_tests` (6), `fuzz_tests` (8), `perf_tests` (2), `backend_wiring_tests` (13), `web_tests` (17)
- `backend_wiring_tests` covers the CLI-to-subsystem seams: `serve` answering over a real socket, and every `db:*` command against a real SQLite file.
- `web_tests` covers the `[[route]]` manifest and the pages it serves: layout wrapping, path parameters, form validation, the 404 page and escaping, again over a real socket.

## Fuzzing (P23)

`tests/fuzz_tests.rs` feeds deterministic (seeded LCG) inputs through lexer/parser/compile_to_nir/bytecode decoder/nxl.toml parser/CLI args with `catch_unwind` panic-hunting. Bytecode decoder also tested for roundtrip fidelity and compilation determinism on valid input. Failure policy: any panic is a GitHub CI defect.

## Performance (P24)

`nxl bench` reports indicative medians for: lexer, parser, compile_to_nir, VM loop (100-iter), NDB migrate-fresh. See the Performance section above.

## CI (P25)

`.github/workflows/ci.yml` — fmt --check, clippy --all-targets, cargo test on ubuntu/macos/windows, cargo build --release. `.github/workflows/release.yml` on tags — cross-build matrix + SHA-256 sums + auto release notes. Run the local mirror: `scripts/ci.ps1`.
