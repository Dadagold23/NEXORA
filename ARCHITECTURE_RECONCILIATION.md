# P02 Architecture Reconciliation

## Executive decision

The repository is a valid Python prototype, but not a production-ready compiler platform. The correct long-term direction is to keep the Python implementation as a reference and validation layer while establishing a Rust-first production compiler pipeline.

## Evidence-based reasoning

### Keep Python as a reference implementation

Python is useful for:

- quick iteration during specification-backed experiments
- building a test oracle against new compiler behavior
- validating semantics that are easier to inspect in a high-level runtime
- developer ergonomics during prototype debugging

Python is not the right production base for:

- compiler performance
- memory safety guarantees
- production runtime stability
- low-level code generation
- native cross-platform backend control
- long-term maintainability of a serious compiler toolchain

### Prefer Rust for the production compiler

Rust is the best first production path because it aligns with the architecture goals:

- memory safety without garbage collection overhead
- strong concurrency primitives for runtime and async work
- excellent ecosystem for command-line tools and compiler infrastructure
- robust FFI boundary management for interop with C, C++, and foreign runtimes
- deterministic performance characteristics suitable for compiler work
- portable cross-compilation support for native and WASM targets

### Maintain NIR as the canonical architecture boundary

The current NIR documentation is valuable and should become the canonical compiler boundary:

```text
NXL source
  -> Lexer / Parser
  -> AST
  -> Semantic / Type Analysis
  -> HIR
  -> NIR
  -> Optimization
  -> Backend
  -> Executable / WASM / Native
```

This keeps the architecture coherent even when the Python prototype is retained for experimentation.

## Recommended architecture

### 1. Reference layer

- Python prototype remains available for exploration, debugging, and regression parity checks
- It is a test/reference interpreter, not the production compiler core
- It should never be the sole authority for final semantics

### 2. Production compiler layer

- Implement a Rust-based compiler core with a formal AST and diagnostics pipeline
- Move semantic analysis and type checking into the production compiler path
- Introduce a real NIR representation with typed instructions and source locations
- Add optimization stages only after the IR is stable

### 3. Initial backend

- First backend target: native machine code for the host platform
- Add WASM later as an explicit second target after the native pipeline is stable
- Keep the backend architecture decoupled from the front-end so additional targets can be added without rewriting the compiler

### 4. Runtime layer

- Build the runtime as a separate subsystem around a stable ABI/API contract
- Keep a minimal runtime surface initially: memory, tasks, filesystem, errors, logging, FFI boundaries
- Expand the runtime only when the compiler and IR stabilize

## Proposed engineering split

| Concern | Production choice | Rationale |
| --- | --- | --- |
| Language frontend | Rust | performance + safety |
| Diagnostics engine | Rust | structured compiler errors |
| Semantic/type analysis | Rust | correctness + formal checks |
| NIR representation | Rust | canonical IR boundary |
| Native backend | Rust | direct performance + cross-compilation |
| Python prototype | Reference only | validation and iteration |
| WASM backend | Later | secondary target after native stability |
| Runtime | Rust/native service | consistent ABI and safety |

## Decision gates

The architecture should only advance when each gate is satisfied:

1. Parser and AST are stable and tested
2. Semantic analysis and type checking are formalized
3. NIR lowering is deterministic and lossless
4. Diagnostics cover errors, warnings, notes, and fix hints
5. Native backend successfully compiles a minimal language subset
6. Runtime ABI is versioned and documented
7. Security, package, and platform boundaries are explicitly defined

## P02 outcome

This phase establishes that the project will not remain a single Python interpreter with aspirational architecture docs. It creates a disciplined split between:

- the prototype as a reference implementation
- the production path as a Rust-first compiler/runtime architecture

This is the correct foundation for P03 and onward.
