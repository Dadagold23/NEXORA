# NXL Rust Compiler

This crate is the production compiler path. The Python implementation in
`tests/` remains an independent reference prototype; it is not a conformance
oracle for syntax or semantics.

## Commands

From this directory, run:

```text
cargo run -- check ..\hello.nxl
cargo run -- nir ..\hello.nxl
cargo run -- parse ..\hello.nxl
cargo run -- --version
cargo test
```

`parse` prints the frontend AST. `check` lexes, parses, resolves names and
annotations, type-checks the supported subset, lowers it to NIR and verifies
that NIR. `nir` prints the NIR-like instruction representation. Unsupported
source forms report `NXL2007` instead of being silently ignored.

## Frontend grammar coverage

The Rust lexer/parser builds AST nodes for the documented source constructs,
including:

- declarations: variables/constants, generic functions, structs, classes, actors, enums, traits/interfaces, aliases, and implementations
- modules/imports/exports; blocks, `if`, `for`, `while`, `loop`, and flow-control statements
- `try`/`catch`/`finally`, `throw`, `yield`, `unsafe`, and guarded `match` with literal, binding, tuple, array, struct, variant, and OR patterns
- primitive/composite/generic/tuple/array/optional/result type annotations and function-level generic `where` constraints
- literals, raw and interpolated strings, arrays, maps, sets, tuples, object construction, closures, async blocks, `await`, `spawn`, channels, calls, indexing, member/null-safe access, assignments, conditionals, and documented operators

Parsing is broader than lowering. `yield`, `unsafe`, `null`, `?.`, map/set/tuple
literals, and tuple/array patterns parse but are not lowered today; they report
`NXL2007`. The executable subset is documented in
`../docs/04-language-reference.md`.

The compiler now has a semantic HIR, a basic-block NIR with a verifier, a
bytecode VM (the authoritative runtime), and JavaScript, Rust and WebAssembly
backends. The VM executes the documented subset end-to-end. The JavaScript
backend covers the documented control-flow and data subset. The Rust backend
emits a small straight-line subset plus `try`/`catch`/`finally` and refuses the
rest. `nxlc backend` emits real target source; it does not generate a native
executable.

Status labels used in the documentation are: Implemented, Partially
implemented, Experimental, Parsed but not lowered, Unsupported, and Not
implemented. The compiler reports one primary diagnostic per compilation with
an optional help; it does not recover to report multiple independent errors.
Passing the ownership checker is not a proof of memory safety: there is no
lifetime inference, NLL, or alias analysis through aggregates, FFI, or
concurrent shared state. See `../PRODUCTION_READINESS.md` for the evidence
matrix.
