# Debugger (P21) and IDE (P22)

## Debugger

```bash
nxl debug <file.nxl>                 # statement trace summary
nxl debug <file.nxl> --break=3       # breakpoint hits in execution order
```

The VM emits `Opcode::DebugSpan(span)` per executed NIR instruction; `debugger::Debugger` maps each span to a source line and reports matched breakpoints (loops count every hit). `VmResult.spans` carries the trace. Line-level step-through reasoning is exposed via the trace; locals lack names after lowering, so watch expressions/variable inspection are NOT IMPLEMENTED.

## IDE

`nxl ide` prints the source-of-truth table — NIDE delegates every capability to the compiler crate, NLS, the formatter, the debugger, and the CLI. `IdeModel` in `compiler/src/ide.rs` exposes diagnostics/outline/completions/hover/definition/format with parity tests against NLS (`compiler/tests/ide_tests.rs`). No second parser/type-checker exists in the IDE layer.
