# Architecture

```mermaid
flowchart TB
    src[NXL source] --> lexer --> parser --> ast --> hir --> nir
    nir --> vm[VM/bytecode] --> debug[debugger]
    nir --> backend[JS backend scaffold]
    stdlib[stdlib] -.-> runtime
    runtime[runtime.rs] -.-> vm
    pkg[pkg.rs / NXP] --> cli[nxlc]
    ndb[ndb.rs / SQLite] -.-> cli
    nuf[nuf.rs / axum] -.-> cli
    nui[nui.rs] --> app[NUI hosts]
    nls[nls.rs] --> cli
    debugger --> cli
    ide --> nls
    ffi --> cli
    security -.-> nuf
    security -.-> ndb
    platforms --> cli
    perf --> cli
    fuzz[fuzz tests] -->|asserts| compiler
    ci[.github/workflows] -->|fmt/clippy/test/build| compiler
```

Key invariants: the compiler crate is the only semantic source of truth (NIDE/NLS/NUF/NDB/NXP are clients of it). Intermediate form `NIR` carries spans used by the bytecode VM's `DebugSpan` opcode. No real native backend yet — `backend.rs` emits JS skeletons; do not treat it as a native target.
