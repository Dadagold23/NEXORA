# Compiler Reference

Pipeline: Lexer (`lexer.rs`) → Parser (`parser.rs`) → AST (`ast.rs`) → HIR/semantic (`hir.rs`) → NIR (`nir.rs`) → VM/backend (`vm.rs`, `backend.rs`).

| Stage | Module | Status |
| --- | --- | --- |
| Source spans & source map | `source.rs` | IMPLEMENTED |
| Structured diagnostics | `diagnostic.rs` | IMPLEMENTED |
| Lexer | `lexer.rs` | IMPLEMENTED for full token subset |
| AST/Parser | `ast.rs`, `parser.rs` | IMPLEMENTED for documented syntax |
| HIR/semantic | `hir.rs` | PARTIAL — block-scoped checks, control-flow context |
| NIR | `nir.rs` | PARTIAL — CFG, limited verifier, instr→span |
| Bytecode & VM | `vm.rs` | IMPLEMENTED (Value, Chunk, CompiledProgram, VmResult.spans) |
| DebugTrace via `Opcode::DebugSpan` | `vm.rs` + `debugger.rs` | IMPLEMENTED (P21) |
| Native/WASM backend | `backend.rs` | SCAFFOLD — JS shell emitter only; NOT a native backend |
| HIR/NIR full conformance | `nir.rs` | NOT IMPLEMENTED (dominance, async metadata, ABI boundaries) |

Errors use structured `Diagnostic` with code/severity/span/help; no raw Python exceptions cross the boundary.
