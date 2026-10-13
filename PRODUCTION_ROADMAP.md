# Production Roadmap

## Current phase

P13 — Package Manager (workflow.txt Module 14)

Status: IMPLEMENTED & TESTED — 266 Rust compiler & package manager tests passing; 13 frontend tests passing; 19 Python prototype tests passing. Ready for P14 Database platform upon approval.

## Current state

The repository has progressed through the production pipeline: frontend parser/lexer (P04), semantic safety (P05), NIR intermediate representation (P06), native/JS backend (P07), runtime execution environment (P08), virtual machine & bytecode engine (P09), async/concurrency (P10), standard library (P11), production CLI (P12), and package manager NXP (P13).

## P02 decisions

- Python remains the prototype/reference implementation
- Rust becomes the production compiler and runtime foundation
- NIR is the canonical compiler boundary
- Native backend is the first target
- WASM backend is deferred until the native path is stable
- The Python interpreter remains useful for parity checking and early validation, not as the final execution engine

## P03 and P04 scope delivered

- `compiler/` is a dependency-free Rust crate
- `nxlc check` parses, performs initial name/type checks, and lowers the supported subset
- `nxlc nir` prints the generated NIR-like instruction form
- `nxlc parse` prints the parsed AST for the expanded language syntax
- `nxlc --version` reports the compiler crate version
- P04 expands lexing and parsing for declarations, generic types/constraints, imports/modules, control flow, error-handling syntax, patterns, collections, closures, string interpolation, async expressions, channels, and operators
- the parser builds ASTs for syntax that NIR lowering does not yet support; such `check` inputs fail with a diagnostic rather than being reported as successfully compiled
- neither phase generates native code or replaces the Python reference toolchain
- Rust compiler and frontend test counts are recorded after the final phase run below
- the existing Python prototype suite passes 19/19 tests

## Phase-numbering reconciliation

`prompt.txt` is the acceptance source for production phases P01–P30. `workflow.txt` uses its own module numbering. See its production-phase crosswalk: workflow Module 06 maps to P05 (type/semantic safety), and workflow Module 07 maps to P06 (NIR). Previously added return/break/continue and branch validation are part of P05 and do not mean P06 is complete.

The current lowerer is only a partial NIR-like representation. P06 remains in progress until the required NIR structure and verification described in `prompt.txt` section 10 are implemented and validated. P06 establishes the backend boundary but does not implement a backend; P07 is the separate backend phase. The shell-text generator in `compiler/src/backend.rs` is a scaffold, not a native backend.

## Approval-gated next action

Continue only within P06 NIR. At its completion, report evidence against the NIR acceptance criteria and stop for explicit approval before beginning P07.

## Recommended production series

1. NXL 0.1 — Language Prototype
2. NXL 0.2 — Compiler Foundation
3. NXL 0.3 — NIR + Backend
4. NXL 0.4 — Runtime
5. NXL 0.5 — Standard Library
6. NXL 0.6 — CLI + Package Manager
7. NXL 0.7 — Database + Web
8. NXL 0.8 — Tooling
9. NXL 0.9 — Beta
10. NXL 0.9 RC
11. NEXORA 1.0

## Release gate guidance

The project must not declare `PRODUCTION READY` based only on code generation or design docs. Production readiness requires sustained evidence across:

- compiler stability
- runtime stability
- security review
- test suite health
- package integrity
- installability
- platform validation
- documentation accuracy

## Approval requirement

Do not proceed to P07 until P06 NIR is complete and explicit approval is granted.
