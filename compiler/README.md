# NXL Rust Compiler Foundation

This crate is the early production compiler path. The Python implementation in `..\nxl\` remains the reference prototype.

## Commands

From this directory, run:

```text
cargo run -- check ..\hello.nxl
cargo run -- nir ..\hello.nxl
cargo run -- parse ..\hello.nxl
cargo run -- --version
cargo test
```

`parse` prints the frontend AST. `check` currently lexes, parses, performs initial undeclared-name and annotation checks, and lowers the supported source subset. `nir` prints that subset's NIR-like instruction representation.

## Frontend grammar coverage

The Rust lexer/parser builds AST nodes for the documented source constructs, including:

- declarations: variables/constants, generic functions, structs, classes, actors, enums, traits/interfaces, aliases, and implementations
- modules/imports/exports; blocks, `if`, `for`, `while`, `loop`, and flow-control statements
- `try`/`catch`/`finally`, `throw`, `yield`, `unsafe`, and guarded `match` with literal, binding, tuple, array, struct, variant, and OR patterns
- primitive/composite/generic/tuple/array/optional/result type annotations and generic `where` constraints
- literals, raw and interpolated strings, arrays, maps, sets, tuples, object construction, closures, async blocks, `await`, `spawn`, channels, calls, indexing, member/null-safe access, assignments, conditionals, and documented operators

Run `nxlc parse <source.nxl>` to inspect the parsed AST. Run `check` or `nir` only for the currently supported lowered subset. The NIR currently includes a basic-block control-flow view and verifies block targets and value references for that subset; it is not yet the full NIR specified by the production design. `nxlc backend <source.nxl>` currently prints a shell-text scaffold from that subset; it does not generate machine code or a functional native executable and is not a production backend.

The grammar frontend does not establish full NXL semantic conformance. Name/type/scope rules, ownership, async/concurrency semantics, module resolution, diagnostics recovery, a production NIR (including explicit basic blocks and verification), native code generation, and runtime execution remain incomplete or future phases. In particular, portions of the prose specification are intentionally represented only as AST syntax and have no execution semantics yet. Under `prompt.txt` phase numbering, control-flow validation is part of P05 semantic safety; P06 is NIR productionization.
