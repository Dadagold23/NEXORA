# Production Readiness Matrix

Scope: this matrix covers the NXL language and compiler toolchain (the
`compiler/` crate). NEXORA subsystems that are not part of the compiler are
listed separately at the end; their detailed evidence lives in
`IMPLEMENTATION_STATUS.md`.

Status vocabulary (used exactly, nowhere else in this file):

- **Implemented** — the feature works end-to-end on the authoritative runtime
  and is covered by automated tests.
- **Partially implemented** — a working subset exists and the gaps are listed.
- **Experimental** — works, but the model or its guarantees are still changing.
- **Parsed but not lowered** — the parser accepts the syntax; compilation
  reports `NXL2007` instead of producing semantics.
- **Unsupported** — deliberately refused or absent, with an explicit
  diagnostic where a source form exists.
- **Not implemented** — no parser, lowering or runtime support exists.

Evidence date: 2026-11-09, working tree based on commit `8572d5f` plus the
Module 2.7 changes (WP-A through WP-E).

## Compiler and language

| Area | Status | Evidence | Known gap |
| --- | --- | --- | --- |
| Lexer | Implemented | `lexes_full_operator_set_and_numeric_conventions`, string/interpolation/comment tests, fuzz seeds | Local evidence is Windows-only |
| Parser | Partially implemented | `language_frontend_tests.rs`; documented forms parse, while `?.`, map/set/tuple literals, tuple/array patterns, `null`, `yield` and `unsafe` are Parsed but not lowered | No multi-error recovery; it stops at the first error |
| HIR and name resolution | Partially implemented | Project/module tests, duplicate and unknown-declaration tests | No privacy (everything public), no re-exports, module declaration is informational |
| Type checker | Partially implemented | `type_system_baseline_tests.rs` (35, including 13 Module 2.7 tests), `generics_tests.rs` (46) | No trait objects, no flow-sensitive typing; dynamic values stay gradual |
| Ownership and borrowing | Partially implemented | `ownership_tests.rs` (48), reference-slot tests (WP-A) | Lexical only: no NLL, no lifetime parameters, no alias analysis through aggregates/FFI/shared state; **not a memory-safety proof** |
| Generic constraints | Partially implemented | `where` validation/enforcement tests (WP-C) | Functions only; no associated types, defaults or lifetime bounds; constraints on type declarations are rejected |
| NIR and verifier | Partially implemented | `compiler_tests.rs` (351), including hand-built negative verifier tests (WP-D) | Verifier covers control flow, definitions, signatures, operand/result types and handler placement for emitted instruction kinds; it is not a semantic proof |
| VM | Implemented | Language/concurrency/VM suites; authoritative execution path | Native execution is not implemented |
| JavaScript backend | Partially implemented | Phase 1-6 parity tests, stdlib parity tests | VM-only modules (`std.http`, `std.networking`, `std.concurrency`), FFI and async are rejected with `NXL2007` |
| Rust backend | Partially implemented | Backend tests when `rustc` is available | Straight-line code plus `try`/`catch`; most constructs are refused with `NXL2007` |
| Concurrency | Experimental | Phase 5 tests (`spawn`, `await`, channels, task groups) | VM only; cancellation is cooperative, with no preemption and no language-level cancellation token |
| Actors | Experimental | Actor/mailbox tests | No built-in scheduler; methods dispatch synchronously |
| C ABI / FFI | Experimental | `cabi_tests.rs`, `ffi_tests.rs` | VM only; scalar signatures are trusted like native linking; the Python bridge requires Python on `PATH` |
| Standard library | Partially implemented | `stdlib_*` and HTTP tests | `std.strings`, `std.math`, `std.crypto` run on VM and JS; `std.http`, `std.networking`, `std.concurrency` are VM-only; the other `std` modules are internal Rust APIs, not NXL builtins |
| CLI | Implemented | `cli_tests` | Behaviour is covered for the documented commands |
| Formatter | Partially implemented | `cli_cmd_fmt_normalises_trailing_whitespace` | Normalizes whitespace only; it is not a full pretty-printer |
| Linter | Not implemented | `cli_parse_lint` | `lint` delegates to the type checker; there are no lint rules |
| Diagnostics | Partially implemented | Diagnostic-code assertions across suites | One primary diagnostic per compilation with an optional help; no multi-error recovery |
| Privacy | Not implemented | Module/type tests | Every declaration is public |

## Semantic limits

- **Memory safety is not claimed.** The checker reasons about lexical scopes
  and binding names. It does not perform alias analysis through aggregates,
  FFI, or concurrent shared state, and there is no lifetime inference or NLL.
- **Reference slots are declared-type based.** `&T`/`&mut T` round-trip
  through declared reference parameters. A reference passed to an
  unannotated generic parameter, an unresolved dynamic call, or any other
  untrackable slot is rejected with `NXL2017` (direct, generic, method,
  indirect and closure calls).
- **`Option`/`Optional` are annotations only.** The two spellings resolve to
  one canonical structural identity, including when nested and through
  imports, and a declared user type shadows the builtin name. There is no
  construction, pattern, or runtime representation, and `null` is not
  lowered.
- **`where` bounds are checked, not erased.** Bounds must name declared
  traits/interfaces, the constrained name must be declared, and every
  instantiation must have a matching `impl`. Inline `<T: Bound>` bounds are
  rejected with a help that points at `where`.
- **The verifier is conservative.** It checks every instruction kind the
  lowerer emits, rejects impossible names/types and malformed handler
  placement, and deliberately leaves runtime-only failures (for example
  calling a non-callable value, or iterating a non-iterable) to the VM as
  catchable `NXL9001` errors.
- **Unreachable kinds and inert metadata are not verified.** `Instruction`
  variants that the lowerer never constructs are NIR vocabulary for future
  backends: they cannot be reached through `nxlc`, so they carry no verifier
  test. `ConcurrencyMetadata` is derived from emitted instructions and is not
  consulted for semantic checks; `Module::foreign_abis` is empty in the current
  lowerer, and `ForeignCall` operands are checked only when present. The
  hand-built WP-D tests call the verifier directly to cover the emitted kinds
  and their negative cases.

## NEXORA subsystems outside this matrix

| Subsystem | Status | Evidence | Known gap |
| --- | --- | --- | --- |
| NDB | Implemented | Driver/ORM/pool/migration tests | Live multi-driver matrix depends on the environment |
| NUF | Implemented | Auth/CSRF/OpenAPI tests | NXL-authored routes are not implemented |
| NUI | Partially implemented | HTML adapter tests | No desktop/WASM adapter |
| NLS | Partially implemented | Workspace index, find-references, rename tests | No semantic code actions |
| NIDE | Partially implemented | IDE tests | Delegates to the compiler model |
| Package manager (NXP) | Partially implemented | Cache/lock/resolution/audit/workspace tests | No hosted registry or package signing |
| Mobile | Partially implemented | Android build tests | iOS is Not implemented; launching needs a device/emulator |
| Release packaging | Partially implemented | Installer/package scripts | Windows is built locally; Linux/macOS are built in CI |

## Verification snapshot

The Module 2.7 verification run is recorded below. Counts are the exact
`cargo test --workspace` result in this working tree; environment-blocked
tests are reported separately rather than counted as passes.

- Focused WP-A: `generics_tests` 46 passed, `ownership_tests` 48 passed.
- Focused WP-B/WP-C: `type_system_baseline_tests` 35 passed, including 13 new
  `module_2_7_*` tests.
- Focused WP-D: `compiler_tests` 351 passed.
- Full workspace: `cargo test --workspace --no-fail-fast` 1025 passed,
  2 failed, 0 ignored across 37 targets. Both failures are `ffi_tests`
  (`python_bridge_calls_functions`, `guest_error_propagates`) and report
  `'python': program not found`; they are environment-blocked, not code
  regressions. Every other target, including the new WP-A..WP-D tests, passed.
- Formatting: `cargo fmt --all -- --check` exits clean.
- Clippy: `cargo clippy --all-targets -- -D warnings` fails on 86
  **pre-existing** findings under Clippy 1.99 (`src/cli.rs` 52, `src/mobile.rs`
  23, `src/nir.rs` 4, `src/stdlib.rs` 3, `src/wasm_backend.rs` 2, `src/vm.rs` 1).
  Every reported location is on a line this module did not change. The one
  warning the new WP-D `Loop` arm would have added was fixed; the rest are left
  untouched to avoid unrelated changes. Because the library findings stop the
  run, integration-test targets are not reached by this command; a separate
  `cargo clippy --tests` (warnings allowed) reports the same library findings
  plus one pre-existing `expect_err` warning in `compiler_tests.rs`. No warning
  originates in lines added for Module 2.7.

Verdict: the compiler is **not** ready for an NXL 1.0 release candidate. The
blockers remain the full type system, native backends, complete NIR semantics,
a production runtime, a hosted registry, NXL-authored server routes, and
broader NUI adapters. No release tag should be cut until those shrink.
