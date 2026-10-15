# Debugger (P21) and IDE (P22)

## Debugger

```bash
nxl debug <file.nxl>                 # statement trace summary
nxl debug <file.nxl> --break=3       # breakpoint hits in execution order
```

The VM emits `Opcode::DebugSpan(span)` per executed NIR instruction; `debugger::Debugger` maps each span to a source line and reports matched breakpoints (loops count every hit). `VmResult.spans` carries the trace. Line-level step-through reasoning is exposed via the trace; locals lack names after lowering, so watch expressions/variable inspection are NOT IMPLEMENTED.

## IDE

`nxl ide` prints the source-of-truth table — NIDE delegates every capability to the compiler crate, NLS, the formatter, the debugger, and the CLI. `IdeModel` in `compiler/src/ide.rs` exposes diagnostics/outline/completions/hover/definition/format with parity tests against NLS (`compiler/tests/ide_tests.rs`). No second parser/type-checker exists in the IDE layer.

## Editors

A VS Code extension lives in [`editors/vscode-nxl`](../editors/vscode-nxl/README.md).

IMPLEMENTED: `.nxl` highlighting, the NXL file icon, Run/Check/Build commands,
and a language server (`nxlc lsp`) providing completion, hover, go-to-definition,
find-references, the symbol outline and diagnostics. Diagnostics come from the
real compiler, so they are the same errors `nxlc check` reports.

NOT IMPLEMENTED: cross-file rename and refactoring, and completions that use
types rather than names — the completion list is the language's keywords plus
the symbols in the open document, and cross-file lookups are lexical rather than
scope-resolved.

The server is `compiler/src/nls.rs`. `scripts/lsp-smoke.js` and
`scripts/lsp-client-check.js` drive it over real stdio, the second one through
the extension's own client, and both run in the test suite.
